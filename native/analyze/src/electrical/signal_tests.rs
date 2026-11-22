use super::*;
use serde_json::{json, Value};
use std::f64::consts::TAU;
fn frozen() -> Value {
    serde_json::from_str::<Value>(include_str!(
        "../../../../tests/fixtures/analysis-signal-kernel/arrays.json"
    ))
    .unwrap()
}
fn gates() -> Value {
    serde_json::from_str::<Value>(include_str!(
        "../../../../tests/fixtures/analysis-signal-kernel/definition.json"
    ))
    .unwrap()["gates"]
        .clone()
}
fn g(key: &str) -> f64 {
    gates()[key].as_f64().unwrap()
}
fn pairs(v: &Value) -> Vec<Complex64> {
    v.as_array()
        .unwrap()
        .iter()
        .map(|z| Complex64::new(z[0].as_f64().unwrap(), z[1].as_f64().unwrap()))
        .collect()
}
fn job() -> Job<'static> {
    Job::new(Instant::now(), None, None).unwrap()
}
// Independent O(N²) reference: no RustFFT plans, scratch, direction or normalization helpers.
fn direct(x: &[Complex64], p: usize, inverse: bool) -> Vec<Complex64> {
    (0..p)
        .map(|k| {
            let mut z = Complex64::default();
            for (n, v) in x.iter().enumerate() {
                let angle = (if inverse { 1. } else { -1. }) * TAU * k as f64 * n as f64 / p as f64;
                z += *v * Complex64::new(angle.cos(), angle.sin());
            }
            if inverse {
                z / p as f64
            } else {
                z
            }
        })
        .collect()
}
fn direct_conv(a: &[Complex64], b: &[Complex64], count: usize) -> Vec<Complex64> {
    (0..count.min(a.len() + b.len() - 1))
        .map(|n| {
            let mut z = Complex64::default();
            for (k, v) in a.iter().enumerate().take(n + 1) {
                if n - k < b.len() {
                    z += *v * b[n - k];
                }
            }
            z
        })
        .collect()
}
fn close(a: f64, b: f64, tol: f64) {
    assert!((a - b).abs() <= tol, "{a:e} != {b:e}, tol {tol:e}");
}
fn maxerr(a: &[Complex64], b: &[Complex64]) -> f64 {
    a.iter()
        .zip(b)
        .map(|(x, y)| (*x - *y).norm())
        .fold(0., f64::max)
}

#[test]
fn frozen_dft_inverse_roundtrip_parseval() {
    let f = frozen();
    for v in f["fixtures"].as_array().unwrap() {
        let x = pairs(&v["samples"]);
        let j = job();
        let fft = transform(&x, false, &j).unwrap();
        let tol = g("dftAbsolute") + g("dftRelativeToInputL1") * v["inputL1"].as_f64().unwrap();
        assert!(
            maxerr(&fft.values, &direct(&x, x.len(), false)) <= tol,
            "{} forward",
            v["name"]
        );
        let inverse = transform(&x, true, &j).unwrap();
        assert!(
            maxerr(&inverse.values, &direct(&x, x.len(), true)) <= tol,
            "{} inverse",
            v["name"]
        );
        let back = transform(&fft.values, true, &j).unwrap();
        assert!(
            maxerr(&back.values, &x)
                <= g("roundtripAbsolute")
                    + g("roundtripRelativeToInputMaximum") * v["inputMaximum"].as_f64().unwrap()
        );
        let energy = v["inputEnergy"].as_f64().unwrap();
        close(
            fft.values.iter().map(|z| z.norm_sqr()).sum::<f64>() / x.len() as f64,
            energy,
            g("energyAbsolute") + g("energyRelative") * energy,
        );
        assert!(
            fft.memory.total_bytes
                >= fft.memory.input_bytes + fft.memory.output_bytes + fft.memory.scratch_bytes
        );
    }
}

#[test]
fn real_amplitude_phase_hz_dc_nyquist_and_units() {
    let f = frozen();
    let j = job();
    assert_eq!(std::mem::size_of::<SpectralBin>(), 40);
    for v in f["fixtures"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|v| v["real"] == true)
    {
        let x: Vec<f64> = pairs(&v["samples"]).iter().map(|z| z.re).collect();
        let dt = v["dtSeconds"].as_f64().unwrap();
        for window in [Window::Rectangular, Window::HannPeriodic] {
            if window == Window::HannPeriodic && (x.len() < 3 || v["rectangularOnly"] == true) {
                continue;
            }
            let s = spectrum(
                Samples::Real(&x),
                dt,
                x.len(),
                window,
                Sides::OneSided,
                Unit::Volt,
                &j,
            )
            .unwrap();
            assert_eq!(s.unit, Unit::Volt);
            assert_eq!(s.unit.symbol(), "V");
            if let Some(k) = v["tone"]["bin"].as_u64() {
                let bin = &s.bins[k as usize];
                let amp = v["tone"]["amplitude"].as_f64().unwrap();
                close(
                    bin.amplitude,
                    amp,
                    g("amplitudeAbsolute") + amp * g("amplitudeRelative"),
                );
                close(
                    bin.coefficient.arg(),
                    v["tone"]["phaseRad"].as_f64().unwrap(),
                    g("phaseRadAbsolute"),
                );
                close(
                    bin.frequency_hz,
                    k as f64 / (dt * x.len() as f64),
                    g("frequencyAbsoluteOverSampleRate") / dt,
                );
                close(
                    s.bins[0].amplitude,
                    v["tone"]["dc"].as_f64().unwrap_or(0.),
                    g("amplitudeAbsolute"),
                );
            }
            if v["name"] == "constant-8" {
                close(s.bins[0].amplitude, 2., g("amplitudeAbsolute"));
            }
            if v["name"] == "nyquist-64" {
                close(s.bins[32].amplitude, 0.8, g("amplitudeAbsolute"));
                close(s.bins[32].frequency_hz, 512., 1e-12);
            }
            if window == Window::HannPeriodic {
                close(s.coherent_gain, 0.5, 1e-14);
                close(s.mean_square_window, 0.375, 1e-14);
                close(
                    s.equivalent_noise_bandwidth_hz,
                    1.5 / (dt * x.len() as f64),
                    1e-11,
                );
            }
            // Window-energy normalization: weighted mean square, not coherent gain.
            let (mut weighted, mut we) = (0., 0.);
            for (n, &a) in x.iter().enumerate() {
                let w = if window == Window::Rectangular {
                    1.
                } else {
                    0.5 - 0.5 * (TAU * n as f64 / x.len() as f64).cos()
                };
                weighted += a * a * w * w;
                we += w * w;
            }
            let expected = weighted / we;
            close(
                s.bins.iter().map(|b| b.psd).sum::<f64>() * s.bin_spacing_hz,
                expected,
                g("energyAbsolute") + g("energyRelative") * expected,
            );
        }
    }
    assert_eq!(Unit::Ampere.symbol(), "A");
    assert_eq!(Unit::Dimensionless.symbol(), "1");
}

#[test]
fn complex_two_sided_psd_padding_and_window_leakage() {
    let f = frozen();
    let j = job();
    for v in f["fixtures"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|v| v["real"] == false)
    {
        let x = pairs(&v["samples"]);
        let s = spectrum(
            Samples::Complex(&x),
            1. / 1024.,
            x.len(),
            Window::Rectangular,
            Sides::TwoSided,
            Unit::Ampere,
            &j,
        )
        .unwrap();
        let e = v["inputEnergy"].as_f64().unwrap() / x.len() as f64;
        close(
            s.bins.iter().map(|b| b.psd).sum::<f64>() * s.bin_spacing_hz,
            e,
            g("energyAbsolute") + g("energyRelative") * e,
        );
        for (k, b) in s.bins.iter().enumerate() {
            close(b.amplitude, b.coefficient.norm() / x.len() as f64, 1e-14);
            let signed = if k <= x.len() / 2 {
                k as f64
            } else {
                k as f64 - x.len() as f64
            };
            close(b.frequency_hz, signed * 1024. / x.len() as f64, 1e-12);
        }
    }
    let v = f["fixtures"]
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["name"] == "offbin-256")
        .unwrap();
    let z = pairs(&v["samples"]);
    let x: Vec<f64> = z.iter().map(|v| v.re).collect();
    let mut leakage = Vec::new();
    for window in [Window::Rectangular, Window::HannPeriodic] {
        let a = spectrum(
            Samples::Real(&x),
            1. / 1024.,
            256,
            window,
            Sides::TwoSided,
            Unit::Volt,
            &j,
        )
        .unwrap();
        let b = spectrum(
            Samples::Real(&x),
            1. / 1024.,
            1024,
            window,
            Sides::TwoSided,
            Unit::Volt,
            &j,
        )
        .unwrap();
        close(b.record_duration_seconds, a.record_duration_seconds, 0.);
        close(b.unpadded_spacing_hz, a.unpadded_spacing_hz, 0.);
        close(b.bin_spacing_hz, a.bin_spacing_hz / 4., 0.);
        close(
            b.equivalent_noise_bandwidth_hz,
            a.equivalent_noise_bandwidth_hz,
            0.,
        );
        let weighted: Vec<Complex64> = z
            .iter()
            .enumerate()
            .map(|(n, &v)| {
                v * if window == Window::Rectangular {
                    1.
                } else {
                    0.5 - 0.5 * (TAU * n as f64 / 256.).cos()
                }
            })
            .collect();
        let reference = direct(&weighted, 1024, false);
        let computed: Vec<Complex64> = b.bins.iter().map(|v| v.coefficient).collect();
        assert!(
            maxerr(&computed, &reference)
                <= g("dftAbsolute") + g("dftRelativeToInputL1") * v["inputL1"].as_f64().unwrap()
        );
        let total = a.bins.iter().map(|b| b.psd).sum::<f64>();
        let outside = a
            .bins
            .iter()
            .enumerate()
            .filter(|(k, _)| {
                ((*k as f64 - 37.35).abs() > 3.) && ((*k as f64 - (256. - 37.35)).abs() > 3.)
            })
            .map(|(_, b)| b.psd)
            .sum::<f64>();
        leakage.push(outside / total);
    }
    assert!(leakage[1] / leakage[0] <= g("hannOutsideMainlobeOverRectangular"));
}

#[test]
fn linear_convolution_direct_dt_and_circular_alias_are_separate() {
    let f = frozen();
    let a = pairs(&f["convolution"]["left"]);
    let b = pairs(&f["convolution"]["right"]);
    let j = job();
    let reference = direct_conv(&a, &b, usize::MAX);
    let scale = a.iter().map(|z| z.norm()).sum::<f64>() * b.iter().map(|z| z.norm()).sum::<f64>();
    let tol = g("convolutionAbsolute") + g("convolutionRelativeToProductL1") * scale;
    for p in [6, 7, 8, 16] {
        for factor in [1., 0.125] {
            let mode = if factor == 1. {
                ConvolutionScale::DiscreteSum
            } else {
                ConvolutionScale::Continuous { dt_seconds: factor }
            };
            let result = convolve(&a, &b, Some(p), mode, (Unit::Volt, Unit::Ampere), &j).unwrap();
            assert_eq!(result.left_unit, Unit::Volt);
            assert_eq!(result.right_unit, Unit::Ampere);
            let scaled: Vec<Complex64> = reference.iter().map(|z| z * factor).collect();
            assert!(maxerr(&result.values, &scaled) <= tol);
            assert_eq!(result.values.capacity(), p);
        }
    }
    let mut padded = b.clone();
    padded.resize(4, Complex64::default());
    let fa = direct(&a, 4, false);
    let fb = direct(&padded, 4, false);
    let product: Vec<Complex64> = fa.iter().zip(fb).map(|(a, b)| a * b).collect();
    let circular = direct(&product, 4, true);
    let expected = [2., 5.5, 3., 4.5];
    for (z, e) in circular.iter().zip(expected) {
        close(z.re, e, tol);
    }
    assert!(convolve(
        &a,
        &b,
        Some(4),
        ConvolutionScale::DiscreteSum,
        (Unit::Volt, Unit::Ampere),
        &j
    )
    .unwrap_err_string()
    .contains("SIGNAL_PADDING"));
    // Complex asymmetric convolution also exercises phase/sign, not just real sums.
    let aa = pairs(&f["fixtures"][7]["samples"]);
    let bb = pairs(&f["fixtures"][10]["samples"]);
    let c = convolve(
        &aa,
        &bb,
        None,
        ConvolutionScale::DiscreteSum,
        (Unit::Dimensionless, Unit::Dimensionless),
        &j,
    )
    .unwrap();
    assert!(
        maxerr(&c.values, &direct_conv(&aa, &bb, usize::MAX))
            <= g("convolutionAbsolute")
                + g("convolutionRelativeToProductL1")
                    * aa.iter().map(|z| z.norm()).sum::<f64>()
                    * bb.iter().map(|z| z.norm()).sum::<f64>()
    );
}
trait ErrorString {
    fn unwrap_err_string(self) -> String;
}
impl<T> ErrorString for Result<T> {
    fn unwrap_err_string(self) -> String {
        match self {
            Err(e) => e,
            Ok(_) => panic!("expected refusal"),
        }
    }
}
#[test]
fn refusals_precede_fft_plans_and_work_buffers() {
    let j = job();
    let x = [Complex64::new(1., 0.)];
    for p in [1, 2, 17, MAX_FFT_LENGTH - 1, MAX_FFT_LENGTH] {
        let bound = memory(p, 16 * p, 16 * p, 16 * p, 2).unwrap();
        assert!(bound.total_bytes < 256 * 1048576);
        assert_eq!(
            bound.total_bytes,
            bound.input_bytes
                + bound.output_bytes
                + bound.work_bytes
                + bound.scratch_bytes
                + bound.plan_bytes
        );
    }
    for scale in [
        ConvolutionScale::Continuous { dt_seconds: -1. },
        ConvolutionScale::Continuous {
            dt_seconds: f64::NAN,
        },
    ] {
        assert!(convolve(&x, &x, None, scale, (Unit::Volt, Unit::Ampere), &j).is_err());
    }
    assert!(convolve(
        &x,
        &x,
        Some(MAX_FFT_LENGTH + 1),
        ConvolutionScale::DiscreteSum,
        (Unit::Volt, Unit::Ampere),
        &j
    )
    .is_err());
    for delta in [0., -1., f64::NAN, f64::INFINITY, f64::MIN_POSITIVE / 1024.] {
        assert!(spectrum(
            Samples::Real(&[1.]),
            delta,
            1,
            Window::Rectangular,
            Sides::OneSided,
            Unit::Volt,
            &j
        )
        .is_err());
    }
    assert!(spectrum(
        Samples::Complex(&x),
        1.,
        1,
        Window::Rectangular,
        Sides::OneSided,
        Unit::Volt,
        &j
    )
    .unwrap_err_string()
    .contains("SIGNAL_SHAPE"));
    assert!(spectrum(
        Samples::Real(&[1.]),
        1.,
        1,
        Window::HannPeriodic,
        Sides::OneSided,
        Unit::Volt,
        &j
    )
    .is_err());
    assert!(spectrum(
        Samples::Real(&[1.]),
        1.,
        MAX_FFT_LENGTH + 1,
        Window::Rectangular,
        Sides::OneSided,
        Unit::Volt,
        &j
    )
    .is_err());
    assert!(spectrum(
        Samples::Real(&[]),
        1.,
        8,
        Window::Rectangular,
        Sides::OneSided,
        Unit::Volt,
        &j
    )
    .is_err());
    for value in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        assert!(transform(&[Complex64::new(value, 0.)], false, &j).is_err());
    }
    assert!(memory(usize::MAX, 0, 0, 0, 1).is_err());
    let tiny = Budget {
        max_seconds: Some(60.),
        max_memory_mi_b: Some(0.001),
    };
    let bounded = Job::new(Instant::now(), Some(&tiny), None).unwrap();
    assert!(transform(&x, false, &bounded)
        .unwrap_err_string()
        .contains("SIGNAL_MEMORY"));
    let flag = AtomicBool::new(true);
    assert!(Job::new(Instant::now(), None, Some(&flag)).is_err());
    flag.store(false, Ordering::Relaxed);
    let cancellable = Job::new(Instant::now(), None, Some(&flag)).unwrap();
    flag.store(true, Ordering::Relaxed);
    assert!(cancellable.check().is_err());
    let expired = Instant::now() - std::time::Duration::from_secs(61);
    assert!(Job::new(expired, None, None)
        .unwrap_err_string()
        .contains("NETWORK_DEADLINE"));
    for limit in [0., -1., f64::NAN, f64::INFINITY, 513.] {
        let b = Budget {
            max_seconds: Some(1.),
            max_memory_mi_b: Some(limit),
        };
        assert!(Job::new(Instant::now(), Some(&b), None).is_err());
    }
    let overflow = [Complex64::new(f64::MAX, 0.); 2];
    assert!(transform(&overflow, false, &j)
        .unwrap_err_string()
        .contains("SIGNAL_FINITE"));
}

#[test]
#[ignore = "one bounded evidence benchmark; explicit output directory required"]
fn bounded_evidence() {
    let out = std::path::PathBuf::from(std::env::var("P6_SIGNAL_EVIDENCE").expect("evidence dir"));
    std::fs::create_dir_all(&out).unwrap();
    let budget = Budget {
        max_seconds: Some(60.),
        max_memory_mi_b: Some(512.),
    };
    let start = Instant::now();
    let j = Job::new(start, Some(&budget), None).unwrap();
    let n = 131072;
    let x: Vec<Complex64> = (0..n)
        .map(|k| {
            Complex64::new(
                0.4 + 1.2 * (TAU * 113. * k as f64 / n as f64 + 0.17).cos()
                    + 0.3 * (TAU * 1733.35 * k as f64 / n as f64).sin(),
                0.,
            )
        })
        .collect();
    let h: Vec<Complex64> = (0..257)
        .map(|k| Complex64::new((-(k as f64) / 30.).exp() / 30., 0.))
        .collect();
    let pair = |z: &Complex64| [z.re, z.im];
    std::fs::write(out.join("inputs.json"),serde_json::to_vec(&json!({"dtSeconds":1./1024.,"samples":x.iter().map(pair).collect::<Vec<_>>(),"kernel":h.iter().map(pair).collect::<Vec<_>>()})).unwrap()).unwrap();
    let t = Instant::now();
    let fft = transform(&x, false, &j).unwrap();
    let fft_s = t.elapsed().as_secs_f64();
    let back = transform(&fft.values, true, &j).unwrap();
    let rt = maxerr(&x, &back.values);
    assert!(
        rt <= g("roundtripAbsolute")
            + g("roundtripRelativeToInputMaximum") * x.iter().map(|z| z.norm()).fold(0., f64::max)
    );
    drop(back);
    drop(fft);
    let t = Instant::now();
    let c = convolve(
        &x,
        &h,
        None,
        ConvolutionScale::DiscreteSum,
        (Unit::Volt, Unit::Dimensionless),
        &j,
    )
    .unwrap();
    let conv_s = t.elapsed().as_secs_f64();
    let t = Instant::now();
    let reference = direct_conv(&x, &h, 512);
    let direct_s = t.elapsed().as_secs_f64();
    let error = maxerr(&c.values[..512], &reference);
    let scale = x.iter().map(|z| z.norm()).sum::<f64>() * h.iter().map(|z| z.norm()).sum::<f64>();
    assert!(error <= g("convolutionAbsolute") + g("convolutionRelativeToProductL1") * scale);
    let real: Vec<f64> = x.iter().map(|z| z.re).collect();
    let s = spectrum(
        Samples::Real(&real),
        1. / 1024.,
        n,
        Window::HannPeriodic,
        Sides::OneSided,
        Unit::Volt,
        &j,
    )
    .unwrap();
    let t = Instant::now();
    let short_reference = direct(&x[..512], 512, false);
    let direct_dft_s = t.elapsed().as_secs_f64();
    let t = Instant::now();
    let short_fft = transform(&x[..512], false, &j).unwrap();
    let short_fft_s = t.elapsed().as_secs_f64();
    let dft_error = maxerr(&short_fft.values, &short_reference);
    let dft_scale = x[..512].iter().map(|z| z.norm()).sum::<f64>();
    assert!(dft_error <= g("dftAbsolute") + g("dftRelativeToInputL1") * dft_scale);
    j.check().unwrap();
    std::fs::write(out.join("arrays.json"),serde_json::to_vec(&json!({"convolution":c.values.iter().map(pair).collect::<Vec<_>>(),"directConvolutionPrefix":reference.iter().map(pair).collect::<Vec<_>>(),"shortDirectDft":short_reference.iter().map(pair).collect::<Vec<_>>(),"shortFft":short_fft.values.iter().map(pair).collect::<Vec<_>>(),"spectrum":s.bins.iter().map(|b|json!([b.frequency_hz,b.coefficient.re,b.coefficient.im,b.amplitude,b.psd])).collect::<Vec<_>>()})).unwrap()).unwrap();
    let metrics = json!({"profile":"Rust debug scalar CPU F64; timing includes plan/scratch", "length":n,"kernelLength":257,"fftSeconds":fft_s,"convolutionSeconds":conv_s,"direct512PrefixSeconds":direct_s,"direct512DftSeconds":direct_dft_s,"fft512Seconds":short_fft_s,"roundtripMax":rt,"convolutionPrefixMax":error,"convolutionErrorScale":scale,"shortDftMax":dft_error,"shortDftErrorScale":dft_scale,"convolutionReservedBytes":c.memory.total_bytes,"spectrumReservedBytes":s.memory.total_bytes,"secondsIncludingSerialization":start.elapsed().as_secs_f64()});
    std::fs::write(
        out.join("metrics.json"),
        serde_json::to_vec_pretty(&metrics).unwrap(),
    )
    .unwrap();
    println!("{metrics}");
}
