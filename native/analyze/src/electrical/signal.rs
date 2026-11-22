//! Bounded finite uniform-sample CPU/F64 primitives; no sampled-model extrapolation.
//! See tests/fixtures/analysis-signal-kernel/README.md for normalization and units.
use super::{deadline, fail, Budget, Result};
pub use rustfft::num_complex::Complex64;
use rustfft::{
    algorithm::{BluesteinsAlgorithm, Radix4},
    Fft, FftDirection,
};
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Instant,
};

pub const MAX_FFT_LENGTH: usize = 262_144;
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Window {
    Rectangular,
    HannPeriodic,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Sides {
    OneSided,
    TwoSided,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Unit {
    Volt,
    Ampere,
    Dimensionless,
}
impl Unit {
    pub fn symbol(self) -> &'static str {
        match self {
            Self::Volt => "V",
            Self::Ampere => "A",
            Self::Dimensionless => "1",
        }
    }
}
#[derive(Clone, Copy)]
pub enum Samples<'a> {
    Real(&'a [f64]),
    Complex(&'a [Complex64]),
}
impl Samples<'_> {
    fn len(self) -> usize {
        match self {
            Self::Real(x) => x.len(),
            Self::Complex(x) => x.len(),
        }
    }
    fn at(self, n: usize) -> Complex64 {
        match self {
            Self::Real(x) => Complex64::new(x[n], 0.),
            Self::Complex(x) => x[n],
        }
    }
    fn bytes(self) -> usize {
        self.len() * if matches!(self, Self::Real(_)) { 8 } else { 16 }
    }
}
#[derive(Clone, Copy, Debug)]
pub enum ConvolutionScale {
    DiscreteSum,
    Continuous { dt_seconds: f64 },
}
#[derive(Clone, Debug)]
pub struct MemoryPlan {
    pub fft_length: usize,
    pub inner_length: usize,
    pub input_bytes: usize,
    pub output_bytes: usize,
    pub work_bytes: usize,
    pub scratch_bytes: usize,
    pub plan_bytes: usize,
    pub total_bytes: usize,
}
/// Same monotonic request clock as electrical jobs. Atomics are cooperative between
/// library calls; the existing disposable worker enforces hard cancellation/deadlines.
pub struct Job<'a> {
    start: Instant,
    budget: Budget,
    cancelled: Option<&'a AtomicBool>,
}
impl<'a> Job<'a> {
    pub fn new(
        start: Instant,
        budget: Option<&Budget>,
        cancelled: Option<&'a AtomicBool>,
    ) -> Result<Self> {
        let budget = budget.cloned().unwrap_or(Budget {
            max_seconds: Some(60.),
            max_memory_mi_b: Some(256.),
        });
        let memory = budget.max_memory_mi_b.unwrap_or(256.);
        if !memory.is_finite() || memory <= 0. || memory > 512. {
            return Err(fail("SIGNAL_BUDGET", "memory must be in (0,512] MiB"));
        }
        let job = Self {
            start,
            budget,
            cancelled,
        };
        job.check()?;
        Ok(job)
    }
    pub fn check(&self) -> Result<()> {
        if self.cancelled.is_some_and(|x| x.load(Ordering::Relaxed)) {
            return Err(fail("SIGNAL_CANCELLED", "request cancelled"));
        }
        deadline(self.start, Some(&self.budget), "signal")
    }
    fn reserve(&self, plan: &MemoryPlan) -> Result<()> {
        self.check()?;
        if plan.total_bytes as f64 > self.budget.max_memory_mi_b.unwrap_or(256.) * 1048576. {
            return Err(fail(
                "SIGNAL_MEMORY",
                "input/output/work/scratch/plan reservation exceeds budget",
            ));
        }
        Ok(())
    }
}
fn length(p: usize) -> Result<usize> {
    if p == 0 || p > MAX_FFT_LENGTH {
        return Err(fail("SIGNAL_LENGTH", "FFT length outside 1..262144"));
    }
    Ok(if p.is_power_of_two() {
        p
    } else {
        (2 * p - 1).next_power_of_two()
    })
}
fn memory(
    p: usize,
    input: usize,
    output: usize,
    work: usize,
    directions: usize,
) -> Result<MemoryPlan> {
    let m = length(p)?;
    let plan = directions * (16 * (8 * m + 4 * p) + 65536);
    let scratch = 32 * m;
    let total = input
        .checked_add(output)
        .and_then(|x| x.checked_add(work))
        .and_then(|x| x.checked_add(plan))
        .and_then(|x| x.checked_add(scratch))
        .ok_or_else(|| fail("SIGNAL_MEMORY", "size overflow"))?;
    Ok(MemoryPlan {
        fft_length: p,
        inner_length: m,
        input_bytes: input,
        output_bytes: output,
        work_bytes: work,
        scratch_bytes: scratch,
        plan_bytes: plan,
        total_bytes: total,
    })
}
fn finite(z: Complex64) -> bool {
    z.re.is_finite() && z.im.is_finite()
}
fn validate(x: Samples<'_>, job: &Job<'_>) -> Result<()> {
    length(x.len())?;
    for n in 0..x.len() {
        if n % 4096 == 0 {
            job.check()?;
        }
        if !finite(x.at(n)) {
            return Err(fail("SIGNAL_FINITE", "nonfinite input sample"));
        }
    }
    Ok(())
}
fn plan(p: usize, dir: FftDirection, job: &Job<'_>) -> Result<Arc<dyn Fft<f64>>> {
    job.check()?;
    let m = length(p)?;
    let inner: Arc<dyn Fft<f64>> = Arc::new(Radix4::<f64>::new(m, dir));
    let fft: Arc<dyn Fft<f64>> = if p == m {
        inner
    } else {
        Arc::new(BluesteinsAlgorithm::<f64>::new(p, inner))
    };
    job.check()?;
    if fft.get_inplace_scratch_len() > 2 * m {
        return Err(fail("SIGNAL_PLAN", "pinned scratch audit violated"));
    }
    Ok(fft)
}
fn run(
    fft: &dyn Fft<f64>,
    x: &mut [Complex64],
    scratch: &mut [Complex64],
    inverse: bool,
    job: &Job<'_>,
) -> Result<()> {
    job.check()?;
    fft.process_with_scratch(x, scratch);
    job.check()?;
    let divisor = if inverse { x.len() as f64 } else { 1. };
    for (n, z) in x.iter_mut().enumerate() {
        if n % 4096 == 0 {
            job.check()?;
        }
        *z /= divisor;
        if !finite(*z) {
            return Err(fail("SIGNAL_FINITE", "FFT overflow"));
        }
    }
    Ok(())
}
pub struct Transform {
    pub values: Vec<Complex64>,
    pub memory: MemoryPlan,
}
/// Forward exp(-i2πkn/N), unnormalized. Inverse exp(+i2πkn/N)/N.
pub fn transform(x: &[Complex64], inverse: bool, job: &Job<'_>) -> Result<Transform> {
    validate(Samples::Complex(x), job)?;
    let mem = memory(x.len(), 16 * x.len(), 16 * x.len(), 0, 1)?;
    job.reserve(&mem)?;
    let fft = plan(
        x.len(),
        if inverse {
            FftDirection::Inverse
        } else {
            FftDirection::Forward
        },
        job,
    )?;
    let mut output = x.to_vec();
    let mut scratch = vec![Complex64::default(); mem.scratch_bytes / 16];
    run(fft.as_ref(), &mut output, &mut scratch, inverse, job)?;
    Ok(Transform {
        values: output,
        memory: mem,
    })
}
#[derive(Debug)]
pub struct SpectralBin {
    pub frequency_hz: f64,
    pub coefficient: Complex64,
    pub amplitude: f64,
    pub psd: f64,
}
pub struct Spectrum {
    pub bins: Vec<SpectralBin>,
    pub sample_count: usize,
    pub fft_length: usize,
    pub dt_seconds: f64,
    pub bin_spacing_hz: f64,
    pub unpadded_spacing_hz: f64,
    pub record_duration_seconds: f64,
    pub coherent_gain: f64,
    pub mean_square_window: f64,
    pub equivalent_noise_bandwidth_hz: f64,
    pub unit: Unit,
    pub sides: Sides,
    pub window: Window,
    pub memory: MemoryPlan,
}
fn dt(dt: f64, p: usize) -> Result<f64> {
    let fs = 1. / dt;
    if !dt.is_finite()
        || dt <= 0.
        || !fs.is_finite()
        || fs / p as f64 == 0.
        || !(dt * p as f64).is_finite()
    {
        return Err(fail(
            "SIGNAL_DT",
            "dt/sample rate/record duration must be finite and positive",
        ));
    }
    Ok(fs)
}
fn weight(w: Window, n: usize, len: usize) -> f64 {
    match w {
        Window::Rectangular => 1.,
        Window::HannPeriodic => 0.5 * (1. - (std::f64::consts::TAU * n as f64 / len as f64).cos()),
    }
}
pub fn spectrum(
    x: Samples<'_>,
    dt_seconds: f64,
    fft_length: usize,
    window: Window,
    sides: Sides,
    unit: Unit,
    job: &Job<'_>,
) -> Result<Spectrum> {
    length(fft_length)?;
    let fs = dt(dt_seconds, fft_length)?;
    if fft_length < x.len()
        || (sides == Sides::OneSided && matches!(x, Samples::Complex(_)))
        || (window == Window::HannPeriodic && x.len() < 3)
    {
        return Err(fail(
            "SIGNAL_SHAPE",
            "padding/one-sided real/Hann length contract violated",
        ));
    }
    validate(x, job)?;
    let bins = if sides == Sides::OneSided {
        fft_length / 2 + 1
    } else {
        fft_length
    };
    let mem = memory(
        fft_length,
        x.bytes(),
        bins * std::mem::size_of::<SpectralBin>(),
        16 * fft_length,
        1,
    )?;
    job.reserve(&mem)?;
    let fft = plan(fft_length, FftDirection::Forward, job)?;
    let mut work = vec![Complex64::default(); fft_length];
    let mut sum = 0.;
    let mut energy = 0.;
    for (n, z) in work[..x.len()].iter_mut().enumerate() {
        if n % 4096 == 0 {
            job.check()?;
        }
        let w = weight(window, n, x.len());
        sum += w;
        energy += w * w;
        *z = x.at(n) * w;
    }
    let mut scratch = vec![Complex64::default(); mem.scratch_bytes / 16];
    run(fft.as_ref(), &mut work, &mut scratch, false, job)?;
    let mut output = Vec::with_capacity(bins);
    for (k, &coefficient) in work[..bins].iter().enumerate() {
        if k % 4096 == 0 {
            job.check()?;
        }
        let fold = if sides == Sides::OneSided
            && k != 0
            && !(fft_length % 2 == 0 && k == fft_length / 2)
        {
            2.
        } else {
            1.
        };
        let amplitude = fold * coefficient.norm() / sum;
        let density_root = coefficient.norm() / fs.sqrt() / energy.sqrt();
        let psd = fold * density_root * density_root;
        if !amplitude.is_finite() || !psd.is_finite() {
            return Err(fail("SIGNAL_FINITE", "spectrum normalization overflow"));
        }
        let index = if k <= fft_length / 2 {
            k as f64
        } else {
            k as f64 - fft_length as f64
        };
        output.push(SpectralBin {
            frequency_hz: index * (fs / fft_length as f64),
            coefficient,
            amplitude,
            psd,
        });
    }
    Ok(Spectrum {
        bins: output,
        sample_count: x.len(),
        fft_length,
        dt_seconds,
        bin_spacing_hz: fs / fft_length as f64,
        unpadded_spacing_hz: fs / x.len() as f64,
        record_duration_seconds: x.len() as f64 * dt_seconds,
        coherent_gain: sum / x.len() as f64,
        mean_square_window: energy / x.len() as f64,
        equivalent_noise_bandwidth_hz: (fs / sum) * (energy / sum),
        unit,
        sides,
        window,
        memory: mem,
    })
}
pub struct Convolution {
    pub values: Vec<Complex64>,
    pub fft_length: usize,
    pub scale: ConvolutionScale,
    pub left_unit: Unit,
    pub right_unit: Unit,
    pub memory: MemoryPlan,
}
pub fn convolve(
    left: &[Complex64],
    right: &[Complex64],
    fft_length: Option<usize>,
    scale: ConvolutionScale,
    units: (Unit, Unit),
    job: &Job<'_>,
) -> Result<Convolution> {
    validate(Samples::Complex(left), job)?;
    validate(Samples::Complex(right), job)?;
    let len = left
        .len()
        .checked_add(right.len())
        .and_then(|x| x.checked_sub(1))
        .ok_or_else(|| fail("SIGNAL_LENGTH", "convolution length overflow"))?;
    length(len)?;
    let p = fft_length.unwrap_or_else(|| len.next_power_of_two());
    length(p)?;
    if p < len {
        return Err(fail(
            "SIGNAL_PADDING",
            "linear convolution requires P >= N+M-1",
        ));
    }
    let factor = match scale {
        ConvolutionScale::DiscreteSum => 1.,
        ConvolutionScale::Continuous { dt_seconds } => {
            dt(dt_seconds, p)?;
            dt_seconds
        }
    };
    let mem = memory(p, 16 * (left.len() + right.len()), 16 * p, 16 * p, 2)?;
    job.reserve(&mem)?;
    let forward = plan(p, FftDirection::Forward, job)?;
    let inverse = plan(p, FftDirection::Inverse, job)?;
    let mut a = vec![Complex64::default(); p];
    let mut b = vec![Complex64::default(); p];
    a[..left.len()].copy_from_slice(left);
    b[..right.len()].copy_from_slice(right);
    let mut scratch = vec![Complex64::default(); mem.scratch_bytes / 16];
    run(forward.as_ref(), &mut a, &mut scratch, false, job)?;
    run(forward.as_ref(), &mut b, &mut scratch, false, job)?;
    for n in 0..p {
        if n % 4096 == 0 {
            job.check()?;
        }
        a[n] *= b[n];
    }
    run(inverse.as_ref(), &mut a, &mut scratch, true, job)?;
    for (n, z) in a.iter_mut().enumerate() {
        if n % 4096 == 0 {
            job.check()?;
        }
        *z *= factor;
        if !finite(*z) {
            return Err(fail("SIGNAL_FINITE", "convolution overflow"));
        }
    }
    a.truncate(len);
    Ok(Convolution {
        values: a,
        fft_length: p,
        scale,
        left_unit: units.0,
        right_unit: units.1,
        memory: mem,
    })
}
#[cfg(test)]
#[path = "signal_tests.rs"]
mod tests;
