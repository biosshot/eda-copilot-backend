//! Native Metal F32 probe tests (issue #17). Opt in with
//! PCB_METAL_PROBE_TESTS=1; they need an Apple GPU and never run in CI.
use cubecl::{prelude::*, wgpu::{WgpuDevice, WgpuRuntime}};
use super::f32_runtime::PcbRuntime;
use super::gpu::{strict_expected as expected, strict_inputs, STRICT_OPS as OPS};
use super::numerics::strict_probe;

const NAMES: [&str; OPS] = ["a+b", "a-b", "a*b", "a/b", "sqrt|a|", "a*b+c", "(a+b)-b", "a!=a",
    "1/a", "-a*0", "a-a", "a*b-a*b", "x*2^126", "a==0", "max(a,b)", "min(a,b)"];

/// (mismatching bits, NaN-only payload differences) per row.
fn compare<R: Runtime>(label: &str, client: &ComputeClient<R>) -> [(usize, usize); OPS] {
    let input = strict_inputs(40_000);
    let cpu = expected(&input);
    let lanes = input.len() / 3;
    let handle = client.create_from_slice(f32::as_bytes(&input));
    let output = client.empty(cpu.len() * 4);
    unsafe {
        strict_probe::launch_unchecked::<R>(client, CubeCount::Static(lanes.div_ceil(64) as u32, 1, 1),
            CubeDim::new_1d(64), ArrayArg::from_raw_parts(handle, input.len()),
            ArrayArg::from_raw_parts(output.clone(), cpu.len()));
    }
    let bytes = client.read_one(output).expect("Metal readback");
    let gpu = f32::from_bytes(&bytes);
    let mut counts = [(0usize, 0usize); OPS];
    let mut shown = [0usize; OPS];
    let mut kinds = [[0usize; 3]; OPS];
    for (index, (&want, &got)) in cpu.iter().zip(gpu).enumerate() {
        if want.to_bits() == got.to_bits() { continue; }
        let op = index % OPS;
        if want.is_nan() && got.is_nan() { counts[op].1 += 1; continue; }
        counts[op].0 += 1;
        let t = &input[(index / OPS) * 3..][..3];
        let kind = if t.iter().any(|v| v.is_subnormal()) { 0 } else if t[0] == 0.0 && t[1] == 0.0 { 1 } else { 2 };
        kinds[op][kind] += 1;
        if shown[op] < 3 {
            shown[op] += 1;
            let t = &input[(index / OPS) * 3..][..3];
            eprintln!("[{label}] {} a={:08x} b={:08x} c={:08x}: CPU={:08x} GPU={:08x}", NAMES[op],
                t[0].to_bits(), t[1].to_bits(), t[2].to_bits(), want.to_bits(), got.to_bits());
        }
    }
    for (op, (bits, nan)) in counts.iter().enumerate() {
        let [subnormal, zeros, other] = kinds[op];
        eprintln!("[{label}] {:>9}: {bits} mismatches (subnormal operand {subnormal}, ±0 pair {zeros}, other {other}), {nan} NaN payload-only differences of {lanes}", NAMES[op]);
    }
    counts
}

fn enabled() -> bool { std::env::var_os("PCB_METAL_PROBE_TESTS").is_some_and(|v| v == "1") }

#[test]
fn strict_msl_runs_backend_probes_and_matches_cpu_bitwise() {
    if !enabled() { return; }
    let client = PcbRuntime::client(&WgpuDevice::DefaultDevice);
    assert_eq!(*client.info(), wgpu::Backend::Metal, "CubeCL did not select native Metal");
    eprintln!("[strict] runtime={}", PcbRuntime::name(&client));
    // Production runs these probes inside the scoped CPU F32 environment.
    let probes = {let _env = crate::float_env::Guard::enter(); super::gpu::validate_client(&super::client::Client::Wgpu(client.clone()))};
    eprintln!("[strict] backend validate_client: {probes:?}");
    let counts = compare("strict", &client);
    probes.expect("backend arithmetic probes on Metal");
    assert!(counts.iter().all(|c| c.0 == 0), "strict Metal differs from CPU F32: {counts:?}");
}

#[test]
fn unmodified_cubecl_msl_is_reported() {
    if !enabled() { return; }
    // Evidence only: CubeCL's MSL as wgpu compiles it, with default fast math.
    let client = WgpuRuntime::client(&WgpuDevice::DefaultDevice);
    assert_eq!(*client.info(), wgpu::Backend::Metal);
    compare("unmodified", &client);
}
