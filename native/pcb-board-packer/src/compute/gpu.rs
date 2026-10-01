//! Shared CubeCL F64 runtime. Adapted from archived compute/gpu.rs (b09f3e5).
//! A single process owns a device lease; all native workers share its client.
use super::workspace::{ScratchKey, Workspace};
use super::{Capabilities, Error, ErrorKind, Requirements};
use cubecl::wgpu::{WgpuDevice, WgpuRuntime};
use cubecl::{client::ComputeClient, prelude::*, server::Handle};
use std::sync::{Mutex, OnceLock};

pub(crate) struct Session {
    pub client: ComputeClient<WgpuRuntime>,
    pub name: String,
    _lease: std::fs::File,
    workspaces: Workspace,
    pub capabilities: Capabilities,
}
enum State {
    New,
    Ready(Session),
    Busy(std::time::Instant),
    Disabled(Error),
}
static STATE: OnceLock<Mutex<State>> = OnceLock::new();
static WAIT_NANOS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
static INITIALIZATIONS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

fn initialize() -> Result<Option<Session>, Error> {
    if std::env::var_os("PCB_BLOCK_GPU_DISABLED").is_some_and(|v| v == "1") {
        return Err(Error::new(
            ErrorKind::Disabled,
            "GPU disabled by PCB_BLOCK_GPU_DISABLED",
        ));
    }
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
        backends: wgpu::Backends::VULKAN,
        ..wgpu::InstanceDescriptor::new_without_display_handle()
    });
    let adapters = cubecl::future::block_on(instance.enumerate_adapters(wgpu::Backends::VULKAN));
    let mut selected = None;
    for kind in [
        wgpu::DeviceType::DiscreteGpu,
        wgpu::DeviceType::IntegratedGpu,
        wgpu::DeviceType::VirtualGpu,
    ] {
        if let Some((index, adapter)) = adapters
            .iter()
            .filter(|a| a.get_info().device_type == kind)
            .enumerate()
            .find(|(_, a)| a.features().contains(wgpu::Features::SHADER_F64))
        {
            selected = Some((
                match kind {
                    wgpu::DeviceType::DiscreteGpu => WgpuDevice::DiscreteGpu(index),
                    wgpu::DeviceType::IntegratedGpu => WgpuDevice::IntegratedGpu(index),
                    _ => WgpuDevice::VirtualGpu(index),
                },
                adapter.get_info(),
            ));
            break;
        }
    }
    let (device, id) =
        selected.ok_or_else(|| Error::new(ErrorKind::NoDevice, "no compatible F64 Vulkan GPU"))?;
    let directory = std::env::temp_dir().join("eda-copilot-gpu");
    std::fs::create_dir_all(&directory)
        .map_err(|e| Error::new(ErrorKind::Lease, format!("GPU lease directory: {e}")))?;
    let lease = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(directory.join(format!("vulkan-{:x}-{:x}.lock", id.vendor, id.device)))
        .map_err(|e| Error::new(ErrorKind::Lease, format!("GPU lease file: {e}")))?;
    match lease.try_lock() {
        Ok(()) => (),
        Err(std::fs::TryLockError::WouldBlock) => return Ok(None),
        Err(std::fs::TryLockError::Error(e)) => {
            return Err(Error::new(ErrorKind::Lease, format!("GPU lease: {e}")))
        }
    }
    let setup = cubecl::wgpu::init_setup::<cubecl::wgpu::Vulkan>(&device, Default::default());
    let actual = setup.adapter.get_info();
    // The probe and runtime must select the same adapter, including its limits.
    if actual.device_type == wgpu::DeviceType::Cpu
        || actual.vendor != id.vendor
        || actual.device != id.device
    {
        return Err(Error::new(
            ErrorKind::AdapterMismatch,
            "GPU probe/runtime adapter mismatch",
        ));
    }
    let client = WgpuRuntime::client(&device);
    let capabilities = Capabilities {
        f64: client
            .features()
            .supports_type(cubecl::ir::ElemType::Float(cubecl::ir::FloatKind::F64)),
        u64: client
            .features()
            .supports_type(cubecl::ir::ElemType::UInt(cubecl::ir::UIntKind::U64)),
    };
    // Preserve the current F64/U64 initialization policy and adapter selection.
    if !capabilities.f64 {
        return Err(Error::new(
            ErrorKind::MissingCapabilities,
            "CubeCL runtime lacks F64",
        ));
    }
    if !capabilities.u64 {
        return Err(Error::new(
            ErrorKind::MissingCapabilities,
            "CubeCL runtime lacks U64 for exact coordinate rounding",
        ));
    }
    // One compatibility check per device. No candidate CPU scoring is involved.
    let mut values: Vec<f64> = (-2048..2048).map(|i| i as f64 / 1000.0).collect();
    values.extend([-0.565, -1.985, 0.0005, -0.0005, 1e12, -1e12, 2e12, -2e12]);
    for n in -64..=64 {
        let middle = (n as f64 + 0.5) / 1000.0;
        values.extend([
            f64::from_bits(middle.to_bits().wrapping_sub(1)),
            middle,
            f64::from_bits(middle.to_bits().wrapping_add(1)),
        ]);
    }
    let input = client.create_from_slice(f64::as_bytes(&values));
    let output = client.empty(values.len() * 8);
    unsafe {
        super::numerics::rounding_probe::launch_unchecked::<WgpuRuntime>(
            &client,
            CubeCount::Static(values.len().div_ceil(128) as u32, 1, 1),
            CubeDim::new_1d(128),
            ArrayArg::from_raw_parts(input, values.len()),
            ArrayArg::from_raw_parts(output.clone(), values.len()),
        );
    }
    let bytes = client
        .read_one(output)
        .map_err(|e| format!("GPU arithmetic probe: {e:?}"))?;
    if let Some((value, got)) = values
        .iter()
        .zip(f64::from_bytes(&bytes))
        .find(|(v, got)| crate::geometry::round_placement(**v) != **got)
    {
        return Err(Error::new(ErrorKind::ArithmeticIncompatibility, format!("GPU coordinate arithmetic failed CPU compatibility probe: input={value:?}, CPU={:?}, GPU={got:?}",crate::geometry::round_placement(*value))));
    }
    let initializations = INITIALIZATIONS.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
    if std::env::var_os("PCB_BLOCK_SOLVER_PROFILE").is_some() {
        eprintln!(
            "[block-gpu-runtime] {}",
            serde_json::json!({"device":actual.name,"initializations":initializations,"precision":"f64"})
        );
    }
    Ok(Some(Session {
        client,
        name: actual.name,
        _lease: lease,
        workspaces: Workspace::default(),
        capabilities,
    }))
}

/// Runs related upload/dispatch/readback under the one process mutex. CPU
/// candidate generation belongs outside. Runtime failures disable this session
/// and release scratch/lease; domain rejections preserve a ready runtime.
/// Report InvalidInput before issuing GPU work. Once work is issued, complete
/// its required readback or return RuntimeFailure so scratch cannot be reused.
pub(crate) fn with_session<T>(
    requirements: Requirements,
    f: impl FnOnce(&mut Session) -> Result<T, Error>,
) -> Result<T, Error> {
    let wait_started = std::time::Instant::now();
    let mut state = STATE
        .get_or_init(|| Mutex::new(State::New))
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    WAIT_NANOS.fetch_add(
        wait_started.elapsed().as_nanos() as u64,
        std::sync::atomic::Ordering::Relaxed,
    );
    // Catch while holding the lock: a runtime panic cannot poison the mutex.
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        if matches!(&*state, State::Busy(until) if std::time::Instant::now() >= *until) {
            *state = State::New;
        }
        if matches!(&*state, State::New) {
            match initialize()? {
                Some(session) => *state = State::Ready(session),
                None => {
                    *state =
                        State::Busy(std::time::Instant::now() + std::time::Duration::from_secs(1))
                }
            }
        }
        match &mut *state {
            State::Ready(s) => {
                s.capabilities.check(requirements)?;
                f(s)
            }
            State::Busy(_) => Err(Error::new(ErrorKind::Busy, "GPU owned by another process")),
            State::Disabled(reason) => Err(reason.clone()),
            State::New => unreachable!(),
        }
    }));
    match result {
        Ok(Ok(value)) => Ok(value),
        failure => {
            let reason = match failure {
                Ok(Err(reason)) => reason,
                Err(payload) => Error::new(
                    ErrorKind::RuntimeFailure,
                    payload
                        .downcast_ref::<String>()
                        .cloned()
                        .or_else(|| payload.downcast_ref::<&str>().map(|s| (*s).into()))
                        .unwrap_or_else(|| "GPU runtime panicked".into()),
                ),
                _ => unreachable!(),
            };
            if !matches!(&*state, State::Busy(_))
                && (reason.disables_runtime() || !matches!(&*state, State::Ready(_)))
            {
                *state = State::Disabled(reason.clone());
            }
            Err(reason)
        }
    }
}

impl Session {
    /// Scratch contents belong to this protected operation only. Cloned handles
    /// must not escape its readback or be stored in an Engine. Resident handles
    /// use client.create_from_slice/empty instead and belong to this runtime.
    pub fn workspace(&mut self, key: ScratchKey, size: usize) -> Handle {
        self.workspaces.buffer(&self.client, key, size)
    }
}

// Runtime-only smoke: each GPU lane handles a separate input, including values
// that cannot be represented in F32. This is not a block scoring benchmark.
#[cfg(any(test, feature = "placement-bench"))]
#[cube(launch_unchecked)]
fn probe_kernel(input: &Array<f64>, output: &mut Array<f64>) {
    let i = ABSOLUTE_POS as usize;
    if i < input.len() {
        output[i] = (input[i] + 0.125) * 2.0;
    }
}

#[cfg(any(test, feature = "placement-bench"))]
pub(crate) fn probe(values: &[f64], inject_panic: bool) -> Result<serde_json::Value, Error> {
    if values.is_empty() || values.iter().any(|v| !v.is_finite()) {
        return Err(Error::new(ErrorKind::InvalidInput, "invalid probe input"));
    }
    with_session(
        Requirements {
            f64: true,
            u64: true,
        },
        |s| {
            if inject_panic {
                panic!("injected GPU runtime panic");
            }
            let input = s.client.create_from_slice(f64::as_bytes(values));
            let output = s.workspace(ScratchKey::new("runtime-probe", 0), values.len() * 8);
            unsafe {
                probe_kernel::launch_unchecked::<WgpuRuntime>(
                    &s.client,
                    CubeCount::Static(values.len().div_ceil(128) as u32, 1, 1),
                    CubeDim::new_1d(128),
                    ArrayArg::from_raw_parts(input, values.len()),
                    ArrayArg::from_raw_parts(output.clone(), values.len()),
                );
            }
            let bytes = s
                .client
                .read_one(output)
                .map_err(|e| format!("GPU readback: {e:?}"))?;
            let result = f64::from_bytes(&bytes)[..values.len()].to_vec();
            if result.iter().any(|v| !v.is_finite()) {
                return Err("nonfinite GPU result".into());
            }
            Ok(
                serde_json::json!({"device":s.name,"precision":"f64","values":result,
            "initializations":INITIALIZATIONS.load(std::sync::atomic::Ordering::Relaxed)}),
            )
        },
    )
}

pub(crate) fn statistics() -> serde_json::Value {
    let state = STATE
        .get_or_init(|| Mutex::new(State::New))
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let bytes = match &*state {
        State::Ready(s) => s.workspaces.bytes(),
        _ => 0,
    };
    serde_json::json!({"initializations":INITIALIZATIONS.load(std::sync::atomic::Ordering::Relaxed),
        "mutexWaitMs":WAIT_NANOS.load(std::sync::atomic::Ordering::Relaxed) as f64/1e6,"workspaceBytes":bytes,"device":match &*state {State::Ready(s)=>Some(&s.name),_=>None},
        "capabilities":match &*state {State::Ready(s)=>Some(s.capabilities),_=>None},
        "state":match &*state {State::New=>"new",State::Ready(_)=>"ready",State::Busy(_)=>"busy",State::Disabled(_)=>"disabled"},
        "unavailableReason":match &*state {State::Busy(_)=>Some(ErrorKind::Busy),State::Disabled(e)=>Some(e.kind),_=>None}})
}

pub(crate) fn ready() -> bool {
    STATE.get().is_some_and(|state| {
        matches!(
            &*state.lock().unwrap_or_else(|e| e.into_inner()),
            State::Ready(_)
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cube(launch_unchecked)]
    fn integer_probe(input: &Array<u32>, output: &mut Array<u32>) {
        let i = ABSOLUTE_POS as usize;
        if i < input.len() {
            output[i] = input[i] * 3 + 7;
        }
    }

    #[test]
    #[ignore = "requires a compatible unleased GPU; run alone with --ignored --exact"]
    fn shared_workspace_alternates_layouts_and_releases_on_failure() {
        let requirements = Requirements {
            f64: true,
            u64: true,
        };
        assert!(!ready());
        assert_eq!(statistics()["initializations"], 0);
        assert_eq!(probe(&[], false).unwrap_err().kind, ErrorKind::InvalidInput);
        assert!(!ready());
        let mut peak = 0;
        for count in [0usize, 1, 129, 4097, 1, 8193, 129] {
            // Two independent consumer layouts deliberately use local slot 0.
            // Both outputs stay live in one operation, proving non-aliasing;
            // subsequent operations alternate growth and reuse of both layouts.
            let floats: Vec<_> = (0..count).map(|i| 16777216.0 + i as f64 * 0.25).collect();
            let tags: Vec<_> = (0..count.div_ceil(2)).map(|i| i as u32).collect();
            with_session(requirements, |s| {
                let output = s.workspace(ScratchKey::new("test-floats", 0), floats.len() * 8);
                let integers = s.workspace(ScratchKey::new("test-tags", 0), tags.len() * 4);
                if count != 0 {
                    let input = s.client.create_from_slice(f64::as_bytes(&floats));
                    let input_tags = s.client.create_from_slice(u32::as_bytes(&tags));
                    unsafe {
                        probe_kernel::launch_unchecked::<WgpuRuntime>(
                            &s.client,
                            CubeCount::Static(count.div_ceil(128) as u32, 1, 1),
                            CubeDim::new_1d(128),
                            ArrayArg::from_raw_parts(input, count),
                            ArrayArg::from_raw_parts(output.clone(), count),
                        );
                        integer_probe::launch_unchecked::<WgpuRuntime>(
                            &s.client,
                            CubeCount::Static(tags.len().div_ceil(128) as u32, 1, 1),
                            CubeDim::new_1d(128),
                            ArrayArg::from_raw_parts(input_tags, tags.len()),
                            ArrayArg::from_raw_parts(integers.clone(), tags.len()),
                        );
                    }
                    let buffers =
                        cubecl::future::block_on(s.client.read_async(vec![output, integers]))
                            .map_err(|e| format!("workspace test readback: {e:?}"))?;
                    assert_eq!(
                        &f64::from_bytes(&buffers[0])[..count],
                        floats.iter().map(|v| (v + 0.125) * 2.0).collect::<Vec<_>>()
                    );
                    assert_eq!(
                        &u32::from_bytes(&buffers[1])[..tags.len()],
                        tags.iter().map(|v| v * 3 + 7).collect::<Vec<_>>()
                    );
                }
                peak = peak.max(count.max(1));
                let expected = (peak * 8).max(8).next_power_of_two()
                    + (peak.div_ceil(2) * 4).max(8).next_power_of_two();
                assert_eq!(s.workspaces.bytes(), expected);
                Ok(())
            })
            .unwrap();
            assert_eq!(statistics()["initializations"], 1);
        }
        let rejected = with_session::<()>(requirements, |_| {
            Err(Error::new(
                ErrorKind::InvalidInput,
                "unsupported test input",
            ))
        })
        .unwrap_err();
        assert_eq!(rejected.kind, ErrorKind::InvalidInput);
        assert!(ready());
        assert_eq!(probe(&[16777216.25], false).unwrap()["initializations"], 1);
        let failed = probe(&[1.0], true).unwrap_err();
        assert_eq!(failed.kind, ErrorKind::RuntimeFailure);
        assert!(!ready());
        assert_eq!(statistics()["workspaceBytes"], 0);
        assert_eq!(
            probe(&[1.0], false).unwrap_err().kind,
            ErrorKind::RuntimeFailure
        );
        assert_eq!(statistics()["initializations"], 1);
        println!("Shared F64/U32 layouts: empty/small/growth/reuse exact; one initialization; input rejection preserves runtime; failure drops scratch and disables retries.");
    }
}
