//! Shared CubeCL F64 runtime. Adapted from archived compute/gpu.rs (b09f3e5).
//! A single process owns a device lease; all native workers share its client.
use cubecl::wgpu::{WgpuDevice, WgpuRuntime};
use cubecl::{client::ComputeClient, prelude::*, server::Handle};
use std::sync::{Mutex, OnceLock};

pub(super) struct Session {
    pub client: ComputeClient<WgpuRuntime>,
    pub name: String,
    _lease: std::fs::File,
    pub workspaces: Vec<Option<(usize, Handle)>>,
}
enum State {
    New,
    Ready(Session),
    Busy(std::time::Instant),
    Disabled(String),
}
static STATE: OnceLock<Mutex<State>> = OnceLock::new();
static WAIT_NANOS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
static INITIALIZATIONS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

fn initialize() -> Result<Option<Session>, String> {
    if std::env::var_os("PCB_BLOCK_GPU_DISABLED").is_some_and(|v| v == "1") {
        return Err("GPU disabled by PCB_BLOCK_GPU_DISABLED".into());
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
    let (device, id) = selected.ok_or("no compatible F64 Vulkan GPU")?;
    let directory = std::env::temp_dir().join("eda-copilot-gpu");
    std::fs::create_dir_all(&directory).map_err(|e| format!("GPU lease directory: {e}"))?;
    let lease = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(directory.join(format!("vulkan-{:x}-{:x}.lock", id.vendor, id.device)))
        .map_err(|e| format!("GPU lease file: {e}"))?;
    match lease.try_lock() {
        Ok(()) => (),
        Err(std::fs::TryLockError::WouldBlock) => return Ok(None),
        Err(std::fs::TryLockError::Error(e)) => return Err(format!("GPU lease: {e}")),
    }
    let setup = cubecl::wgpu::init_setup::<cubecl::wgpu::Vulkan>(&device, Default::default());
    let actual = setup.adapter.get_info();
    // The probe and runtime must select the same adapter, including its limits.
    if actual.device_type == wgpu::DeviceType::Cpu
        || actual.vendor != id.vendor
        || actual.device != id.device
    {
        return Err("GPU probe/runtime adapter mismatch".into());
    }
    let client = WgpuRuntime::client(&device);
    if !client
        .features()
        .supports_type(cubecl::ir::ElemType::Float(cubecl::ir::FloatKind::F64))
    {
        return Err("CubeCL runtime lacks F64".into());
    }
    if !client
        .features()
        .supports_type(cubecl::ir::ElemType::UInt(cubecl::ir::UIntKind::U64))
    {
        return Err("CubeCL runtime lacks U64 for exact coordinate rounding".into());
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
        super::gpu_kernels::rounding_probe::launch_unchecked::<WgpuRuntime>(
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
        return Err(format!("GPU coordinate arithmetic failed CPU compatibility probe: input={value:?}, CPU={:?}, GPU={got:?}",crate::geometry::round_placement(*value)));
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
        workspaces: Vec::new(),
    }))
}

pub(super) fn with_session<T>(
    f: impl FnOnce(&mut Session) -> Result<T, String>,
) -> Result<T, String> {
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
            State::Ready(s) => f(s),
            State::Busy(_) => Err("GPU owned by another process".into()),
            State::Disabled(reason) => Err(reason.clone()),
            State::New => unreachable!(),
        }
    }));
    match result {
        Ok(Ok(value)) => Ok(value),
        failure => {
            let reason = match failure {
                Ok(Err(reason)) => reason,
                Err(payload) => payload
                    .downcast_ref::<String>()
                    .cloned()
                    .or_else(|| payload.downcast_ref::<&str>().map(|s| (*s).into()))
                    .unwrap_or_else(|| "GPU runtime panicked".into()),
                _ => unreachable!(),
            };
            if !matches!(&*state, State::Busy(_)) {
                *state = State::Disabled(reason.clone());
            }
            Err(reason)
        }
    }
}

impl Session {
    pub fn workspace(&mut self, slot: usize, size: usize) -> Handle {
        self.workspaces
            .resize_with(self.workspaces.len().max(slot + 1), || None);
        let entry = &mut self.workspaces[slot];
        if entry
            .as_ref()
            .is_none_or(|(capacity, _)| *capacity < size.max(8))
        {
            let capacity = size.max(8).next_power_of_two();
            *entry = Some((capacity, self.client.empty(capacity)));
        }
        entry.as_ref().unwrap().1.clone()
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
pub(crate) fn probe(values: &[f64], inject_panic: bool) -> Result<serde_json::Value, String> {
    if values.is_empty() || values.iter().any(|v| !v.is_finite()) {
        return Err("invalid probe input".into());
    }
    with_session(|s| {
        if inject_panic {
            panic!("injected GPU runtime panic");
        }
        let input = s.client.create_from_slice(f64::as_bytes(values));
        let output = s.workspace(0, values.len() * 8);
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
    })
}

pub(super) fn statistics() -> serde_json::Value {
    let state = STATE
        .get_or_init(|| Mutex::new(State::New))
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let bytes = match &*state {
        State::Ready(s) => s
            .workspaces
            .iter()
            .flatten()
            .map(|(size, _)| size)
            .sum::<usize>(),
        _ => 0,
    };
    serde_json::json!({"initializations":INITIALIZATIONS.load(std::sync::atomic::Ordering::Relaxed),
        "mutexWaitMs":WAIT_NANOS.load(std::sync::atomic::Ordering::Relaxed) as f64/1e6,"workspaceBytes":bytes})
}

pub(super) fn ready() -> bool {
    STATE.get().is_some_and(|state| {
        matches!(
            &*state.lock().unwrap_or_else(|e| e.into_inner()),
            State::Ready(_)
        )
    })
}
