//! Shared CubeCL F32 runtime. Adapted from archived compute/gpu.rs (b09f3e5).
//! A single process owns a device lease; all native workers share its client.
use super::workspace::{ScratchKey, Workspace};
use super::{Capabilities, Error, ErrorKind, Requirements};
use cubecl::wgpu::WgpuDevice;
use super::f32_runtime::PcbRuntime as WgpuRuntime;
use cubecl::{client::ComputeClient, prelude::*, server::Handle};
use std::sync::{Arc, Mutex, OnceLock};

pub(crate) struct Session {
    pub client: ComputeClient<WgpuRuntime>,
    pub name: String,
    _lease: Arc<std::fs::File>,
    workspaces: Workspace,
    in_flight: bool,
    pub capabilities: Capabilities,
    float_controls: serde_json::Value,
    memory: Arc<super::memory::Monitor>,
}
enum State {
    New,
    Ready(Session),
    Busy(std::time::Instant),
    Disabled(Error),
}
static STATE: OnceLock<Mutex<State>> = OnceLock::new();
static IDLE_WORKSPACES: Mutex<Vec<Workspace>> = Mutex::new(Vec::new());
static ACTIVE_BYTES: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
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
            .next()
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
        selected.ok_or_else(|| Error::new(ErrorKind::NoDevice, "no compatible F32 Vulkan GPU"))?;
    let float_controls = unsafe {
        let adapter = adapters.iter().find(|a| {
            let actual=a.get_info();actual.vendor==id.vendor&&actual.device==id.device&&actual.device_type==id.device_type
        }).ok_or_else(|| Error::new(ErrorKind::AdapterMismatch,"selected Vulkan adapter disappeared"))?;
        let hal=adapter.as_hal::<wgpu::hal::api::Vulkan>().ok_or_else(|| Error::new(ErrorKind::NoDevice,"Vulkan HAL unavailable"))?;
        let mut controls=ash::vk::PhysicalDeviceFloatControlsProperties::default();
        let mut properties=ash::vk::PhysicalDeviceProperties2::default().push_next(&mut controls);
        hal.shared_instance().raw_instance().get_physical_device_properties2(hal.raw_physical_device(),&mut properties);
        serde_json::json!({"rte":controls.shader_rounding_mode_rte_float32!=0,
            "denormPreserve":controls.shader_denorm_preserve_float32!=0,
            "denormFlushToZero":controls.shader_denorm_flush_to_zero_float32!=0,
            "signedZeroInfNanPreserve":controls.shader_signed_zero_inf_nan_preserve_float32!=0,
            "denormIndependence":controls.denorm_behavior_independence.as_raw(),
            "roundingIndependence":controls.rounding_mode_independence.as_raw()})
    };
    if ["rte","signedZeroInfNanPreserve"].iter().any(|key|float_controls[key]!=true) {
        return Err(Error::new(ErrorKind::MissingCapabilities,format!("F32 float controls unavailable: {float_controls}")));
    }
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
    let client = WgpuRuntime::client(&device);
    if *client.info()!=wgpu::Backend::Vulkan {
        return Err(Error::new(ErrorKind::AdapterMismatch,"PCB F32 float controls were probed for Vulkan but CubeCL selected a different backend"));
    }
    let capabilities = Capabilities {
        f32: client
            .features()
            .supports_type(cubecl::ir::ElemType::Float(cubecl::ir::FloatKind::F32)),
        u64: client
            .features()
            .supports_type(cubecl::ir::ElemType::UInt(cubecl::ir::UIntKind::U64)),
    };
    // Preserve the current F32/U64 initialization policy and adapter selection.
    if !capabilities.f32 {
        return Err(Error::new(
            ErrorKind::MissingCapabilities,
            "CubeCL runtime lacks F32",
        ));
    }
    // One compatibility check per device. No candidate CPU scoring is involved.
    let mut values: Vec<f32> = (-2048..2048).map(|i| i as f32 / 1000.0).collect();
    values.extend([-0.565, -1.985, 0.0005, -0.0005, 1024.0, -1024.0, 512.0, -512.0]);
    for n in -64..=64 {
        let middle = (n as f32 + 0.5) / 1000.0;
        values.extend([
            f32::from_bits(middle.to_bits().wrapping_sub(1)),
            middle,
            f32::from_bits(middle.to_bits().wrapping_add(1)),
        ]);
    }
    let input = client.create_from_slice(f32::as_bytes(&values));
    let output = client.empty(values.len() * 4);
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
        .zip(f32::from_bytes(&bytes))
        .find(|(v, got)| crate::geometry::round_placement(**v) != **got)
    {
        return Err(Error::new(ErrorKind::ArithmeticIncompatibility, format!("GPU coordinate arithmetic failed CPU compatibility probe: input={value:?}, CPU={:?}, GPU={got:?}",crate::geometry::round_placement(*value))));
    }
    let values = [0.0f32, -0.0, f32::from_bits(1), -f32::from_bits(1),
        f32::from_bits(0x007fffff), -f32::from_bits(0x007fffff),
        f32::MIN_POSITIVE, -f32::MIN_POSITIVE,
        f32::from_bits(0x3f800001), f32::from_bits(0x3f7ffffe)];
    let mut expected: Vec<f32> = values.iter().flat_map(|&x| [x * 2.0, x * 0.5, x * 2.0f32.powi(126)]).collect();
    expected.push(values[values.len()-2] * values[values.len()-1] - 1.0);
    let input = client.create_from_slice(f32::as_bytes(&values));
    let output = client.empty(expected.len() * 4);
    unsafe {
        super::numerics::execution_probe::launch_unchecked::<WgpuRuntime>(
            &client, CubeCount::Static(1, 1, 1), CubeDim::new_1d(32),
            ArrayArg::from_raw_parts(input, values.len()),
            ArrayArg::from_raw_parts(output.clone(), expected.len()));
    }
    let bytes = client.read_one(output).map_err(|e| format!("GPU FTZ probe: {e:?}"))?;
    for (index, (&cpu, &actual)) in expected.iter().zip(f32::from_bytes(&bytes)).enumerate() {
        if cpu.to_bits() != actual.to_bits() {
            return Err(Error::new(ErrorKind::ArithmeticIncompatibility,
                format!("F32 execution probe {index}: CPU={:08x} GPU={:08x}", cpu.to_bits(), actual.to_bits())));
        }
    }
    // RTE float controls do not make Vulkan division/sqrt correctly rounded.
    // Check their specified precision with live inputs and outward bounds.
    let values=[0.0f32,-0.0,0.00025,0.001,0.3,1.0,2.0,512.0,1024.0,
        f32::MIN_POSITIVE,-f32::MIN_POSITIVE,f32::from_bits(1)];
    let input=client.create_from_slice(f32::as_bytes(&values));
    let output=client.empty(values.len()*2*4);
    unsafe {super::numerics::arithmetic_probe::launch_unchecked::<WgpuRuntime>(
        &client,CubeCount::Static(1,1,1),CubeDim::new_1d(32),
        ArrayArg::from_raw_parts(input,values.len()),ArrayArg::from_raw_parts(output.clone(),values.len()*2));}
    let bytes=client.read_one(output).map_err(|e|format!("GPU division/sqrt probe: {e:?}"))?;
    let actual=f32::from_bytes(&bytes);
    for (i,&value) in values.iter().enumerate() {
        use crate::interval::Interval as I;
        let division=I::exact(value).div_vulkan(I::exact(1.3));
        let root=I::exact(value.abs()).sqrt_vulkan();
        if !division.contains(actual[i*2]) || !root.contains(actual[i*2+1]) {
            return Err(Error::new(ErrorKind::ArithmeticIncompatibility,format!("GPU division/sqrt precision probe failed for input {value}")));
        }
    }
    let initializations = INITIALIZATIONS.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
    if std::env::var_os("PCB_BLOCK_SOLVER_PROFILE").is_some() {
        eprintln!(
            "[block-gpu-runtime] {}",
            serde_json::json!({"device":id.name,"initializations":initializations,"precision":"f32","floatControls":float_controls})
        );
    }
    let adapter=adapters.into_iter().find(|a| {
        let info=a.get_info();info.vendor==id.vendor&&info.device==id.device&&info.device_type==id.device_type
    }).ok_or_else(||Error::new(ErrorKind::AdapterMismatch,"budget adapter disappeared"))?;
    let memory=Arc::new(super::memory::Monitor::new(super::memory::VulkanBudget::new(adapter)));
    Ok(Some(Session {
        client,
        name: id.name,
        _lease: Arc::new(lease),
        workspaces: Workspace::default(),
        in_flight: false,
        capabilities,
        float_controls,
        memory,
    }))
}

/// Admit through the shared FIFO queue, then own scratch until readback completes.
/// The device-state mutex protects initialization/failure, never GPU execution.
pub(crate) fn with_session<T>(
    requirements: Requirements,
    f: impl FnOnce(&mut Session) -> Result<T, Error>,
) -> Result<T, Error> {
    with_batch(requirements,"setup",0,f)
}

pub(crate) fn with_batch<T>(requirements:Requirements,class:&'static str,work:usize,
    f:impl FnOnce(&mut Session)->Result<T,Error>)->Result<T,Error> {
    let _float_env = crate::float_env::Guard::enter();
    if let Some(memory)=memory_monitor() {super::queue::observe_memory(memory.snapshot().map(|b|b.usable()),class,0);}
    let _permit=super::queue::acquire(class,work);
    let wait_started=std::time::Instant::now();
    let state_mutex=STATE.get_or_init(||Mutex::new(State::New));
    let mut state=state_mutex.lock().unwrap_or_else(|e|e.into_inner());
    WAIT_NANOS.fetch_add(wait_started.elapsed().as_nanos() as u64,std::sync::atomic::Ordering::Relaxed);
    if matches!(&*state,State::Busy(until) if std::time::Instant::now()>=*until) {*state=State::New;}
    if matches!(&*state,State::New) {
        let initialized=std::panic::catch_unwind(std::panic::AssertUnwindSafe(initialize));
        match initialized {
            Ok(Ok(Some(session)))=>*state=State::Ready(session),
            Ok(Ok(None))=>*state=State::Busy(std::time::Instant::now()+std::time::Duration::from_secs(1)),
            Ok(Err(error))=>{*state=State::Disabled(error.clone());return Err(error);},
            Err(_)=>{let error=Error::new(ErrorKind::RuntimeFailure,"GPU initialization panicked");*state=State::Disabled(error.clone());return Err(error);}
        }
    }
    let mut session=match &*state {
        State::Ready(s)=>{
            s.capabilities.check(requirements)?;
            Session {client:s.client.clone(),name:s.name.clone(),_lease:s._lease.clone(),
                capabilities:s.capabilities,float_controls:s.float_controls.clone(),memory:s.memory.clone(),in_flight:true,
                workspaces:IDLE_WORKSPACES.lock().unwrap_or_else(|e|e.into_inner()).pop().unwrap_or_default()}
        },
        State::Busy(_)=>return Err(Error::new(ErrorKind::Busy,"GPU owned by another process")),
        State::Disabled(error)=>return Err(error.clone()),
        State::New=>unreachable!(),
    };
    drop(state);
    let initial_bytes=session.workspaces.bytes();
    ACTIVE_BYTES.fetch_add(initial_bytes,std::sync::atomic::Ordering::Relaxed);
    let attempted=std::panic::catch_unwind(std::panic::AssertUnwindSafe(||f(&mut session)));
    ACTIVE_BYTES.fetch_sub(session.workspaces.bytes(),std::sync::atomic::Ordering::Relaxed);
    super::queue::observe_memory(session.memory.snapshot().map(|b|b.usable()),class,session.workspaces.bytes());
    let result=match attempted {
        Ok(result)=>result,
        Err(payload)=>Err(Error::new(ErrorKind::RuntimeFailure,
            payload.downcast_ref::<String>().cloned().or_else(||payload.downcast_ref::<&str>().map(|s|s.to_string()))
                .unwrap_or_else(||"GPU runtime panicked".into())))
    };
    let mut state=state_mutex.lock().unwrap_or_else(|e|e.into_inner());
    if let Err(error)=&result {
        if error.disables_runtime() {
            *state=State::Disabled(error.clone());
            IDLE_WORKSPACES.lock().unwrap_or_else(|e|e.into_inner()).clear();
        }
    }
    // Another in-flight operation may have disabled the device. Never publish
    // its sibling's partial result after a session failure.
    if let State::Disabled(error)=&*state {return Err(error.clone());}
    if matches!(&*state,State::Ready(_)) {
        IDLE_WORKSPACES.lock().unwrap_or_else(|e|e.into_inner()).push(session.workspaces);
    }
    result
}

fn memory_monitor()->Option<Arc<super::memory::Monitor>> {
    let state=STATE.get()?.lock().unwrap_or_else(|e|e.into_inner());
    match &*state {State::Ready(s)=>Some(s.memory.clone()),_=>None}
}

impl Session {
    /// No fixed application quota. Unknown telemetry falls back to the actual
    /// backend allocation limit, never to a fabricated free-memory value.
    pub fn allocation_budget(&self)->usize {
        let limit=self.client.properties().memory.max_page_size as usize;
        self.memory.snapshot().map_or(limit,|b|limit.min(b.usable().min(usize::MAX as u64) as usize))
    }

    /// Scratch contents belong to this protected operation only. Cloned handles
    /// must not escape its readback or be stored in an Engine. Resident handles
    /// use client.create_from_slice/empty instead and belong to this runtime.
    pub fn workspace(&mut self, key: ScratchKey, size: usize) -> Handle {
        let before=self.workspaces.bytes();
        let handle=self.workspaces.buffer(&self.client, key, size);
        if self.in_flight {ACTIVE_BYTES.fetch_add(self.workspaces.bytes()-before,std::sync::atomic::Ordering::Relaxed);}
        handle
    }
}

// Runtime-only smoke: each GPU lane handles a separate input, including values
// that exercise binary32 arithmetic. This is not a block scoring benchmark.
#[cfg(any(test, feature = "placement-bench"))]
#[cube(launch_unchecked)]
fn probe_kernel(input: &Array<f32>, output: &mut Array<f32>) {
    let i = ABSOLUTE_POS as usize;
    if i < input.len() {
        output[i] = (input[i] + 0.125) * 2.0;
    }
}

#[cfg(any(test, feature = "placement-bench"))]
pub(crate) fn probe(values: &[f32], inject_panic: bool) -> Result<serde_json::Value, Error> {
    if values.is_empty() || values.iter().any(|v| !v.is_finite()) {
        return Err(Error::new(ErrorKind::InvalidInput, "invalid probe input"));
    }
    with_session(
        Requirements {
            f32: true,
            u64: false,
        },
        |s| {
            if inject_panic {
                panic!("injected GPU runtime panic");
            }
            let input = s.client.create_from_slice(f32::as_bytes(values));
            let output = s.workspace(ScratchKey::new("runtime-probe", 0), values.len() * 4);
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
            let result = f32::from_bytes(&bytes)[..values.len()].to_vec();
            if result.iter().any(|v| !v.is_finite()) {
                return Err("nonfinite GPU result".into());
            }
            Ok(
                serde_json::json!({"device":s.name,"precision":"f32","values":result,
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
        State::Ready(_) => IDLE_WORKSPACES.lock().unwrap_or_else(|e|e.into_inner()).iter().map(Workspace::bytes).sum::<usize>() + ACTIVE_BYTES.load(std::sync::atomic::Ordering::Relaxed),
        _ => 0,
    };
    serde_json::json!({"queue":super::queue::statistics(),"initializations":INITIALIZATIONS.load(std::sync::atomic::Ordering::Relaxed),
        "mutexWaitMs":WAIT_NANOS.load(std::sync::atomic::Ordering::Relaxed) as f64/1e6,"workspaceBytes":bytes,"device":match &*state {State::Ready(s)=>Some(&s.name),_=>None},
        "capabilities":match &*state {State::Ready(s)=>Some(s.capabilities),_=>None},
        "precision":"f32-rte-ftz-v1",
        "kernelPreparation":super::startup::statistics(),
        "kernelCache":super::kernel_cache::statistics(),
        "memory":match &*state {State::Ready(s)=>s.memory.report(),_=>serde_json::Value::Null},
        "floatControls":match &*state {State::Ready(s)=>Some(&s.float_controls),_=>None},
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
    fn concurrent_operations_own_scratch_until_readback() {
        let barrier=std::sync::Arc::new(std::sync::Barrier::new(4));
        std::thread::scope(|scope| {
            let handles:Vec<_>=(0..4).map(|job| {
                let barrier=barrier.clone();
                scope.spawn(move || {
                    with_session(Requirements {f32:true,u64:false},|s| {
                        let values=vec![job as f32+0.25;257];
                        let input=s.client.create_from_slice(f32::as_bytes(&values));
                        let output=s.workspace(ScratchKey::new("concurrent-probe",0),257*4);
                        // All four operations own their buffers before any launch.
                        barrier.wait();
                        unsafe {probe_kernel::launch_unchecked::<WgpuRuntime>(&s.client,
                            CubeCount::Static(3,1,1),CubeDim::new_1d(128),
                            ArrayArg::from_raw_parts(input,257),ArrayArg::from_raw_parts(output.clone(),257));}
                        let bytes=s.client.read_one(output).map_err(|e|format!("concurrent read: {e:?}"))?;
                        assert!(f32::from_bytes(&bytes)[..257].iter().all(|&v|v==(job as f32+0.375)*2.0));
                        Ok(())
                    }).unwrap();
                })
            }).collect();
            for handle in handles {handle.join().unwrap();}
        });
        assert_eq!(statistics()["queue"]["active"],0);
        assert_eq!(statistics()["initializations"],1);
        let report=statistics();
        assert!(report["memory"]["known"].is_boolean());
        if report["memory"]["known"]==true {
            let budget=report["memory"]["budgetBytes"].as_u64().unwrap();
            let usage=report["memory"]["usageBytes"].as_u64().unwrap();
            assert_eq!(report["memory"]["headroomBytes"].as_u64(),Some(budget.saturating_sub(usage)));
        } else {assert!(report["memory"]["headroomBytes"].is_null());}
        eprintln!("gpu-memory-probe: {}",report["memory"]);
    }

    #[test]
    #[ignore = "requires a compatible unleased GPU; run alone with --ignored --exact"]
    fn shared_workspace_alternates_layouts_and_releases_on_failure() {
        let requirements = Requirements {
            f32: true,
            u64: false,
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
            let floats: Vec<_> = (0..count).map(|i| 128.0 + i as f32 * 0.25).collect();
            let tags: Vec<_> = (0..count.div_ceil(2)).map(|i| i as u32).collect();
            with_session(requirements, |s| {
                let output = s.workspace(ScratchKey::new("test-floats", 0), floats.len() * 4);
                let integers = s.workspace(ScratchKey::new("test-tags", 0), tags.len() * 4);
                if count != 0 {
                    let input = s.client.create_from_slice(f32::as_bytes(&floats));
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
                        &f32::from_bytes(&buffers[0])[..count],
                        floats.iter().map(|v| (v + 0.125) * 2.0).collect::<Vec<_>>()
                    );
                    assert_eq!(
                        &u32::from_bytes(&buffers[1])[..tags.len()],
                        tags.iter().map(|v| v * 3 + 7).collect::<Vec<_>>()
                    );
                }
                peak = peak.max(count.max(1));
                let expected = (peak * 4).max(8).next_power_of_two()
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
        println!("Shared F32/U32 layouts: empty/small/growth/reuse exact; one initialization; input rejection preserves runtime; failure drops scratch and disables retries.");
    }
}
