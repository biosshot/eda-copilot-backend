# P6 internal signal kernel — review packet

Ready for review, uncommitted. Scope is the internal finite uniform-sample CPU/F64 primitive; it does not close all P6/P11/EMI. Production entry: `native/analyze/src/electrical/signal.rs`, tests `signal_tests.rs`; only registration plus Cargo dependency/locks change outside that module. Accepted resonant runtime, physical/coupled energy guards and placement F32/GPU code are preserved. All 124 previous runtime-manifest files outside the five intentionally extended shared files remain byte-identical; no accepted ignored evidence was removed.

The full normalization, units, supported shapes, preallocation admission, cancellation limits, dependency rationale and reproduction commands are in [fixture README](../../../tests/fixtures/analysis-signal-kernel/README.md). Forward is unscaled negative-exponent DFT; inverse divides by length once. Real one-sided amplitude and PSD fold interior bins only; periodic Hann coherent gain and window energy are distinct. Linear convolution has explicit discrete/continuous-dt selection and refuses insufficient padding. Zero padding does not extend physical information.

## Frozen gates and checks

Input-only gates were frozen before numerical calculations. `definition.json` SHA256 `7fe028f2c462189f0c288f4175f807a9ec57c5cba70264bf7b4cd9f6c2e978a5`; `arrays.json` SHA256 `2e4290dd884346c0fc385563aaf50367e17389bbdac33c2112dd1bb174f5a926`. Neither changed during repairs/runs.

| Check | Evidence |
|---|---|
| 17 frozen vectors: impulse, constant, bin/off-bin tones, complex asymmetric, deterministic random, odd/even/prime | Independent O(N²) forward/inverse DFT, roundtrip and Parseval PASS |
| Amplitude, phase, Hz, DC/even Nyquist/odd last bin, units | Rectangular and periodic-Hann tests PASS; isolated-tone qualifications explicit |
| PSD integral, coherent gain, window energy/ENBW | Real one-sided and complex two-sided PASS |
| Hann leakage outside mainlobe vs rectangular | Frozen ratio <=0.1 PASS |
| Zero-padded Fourier interpolation | Independent direct DFT PASS; record duration/unpadded spacing/ENBW unchanged |
| Linear convolution, dt once, complex sign, circular aliasing separately | Direct sums at P=6/7/8/16 PASS; insufficient P rejected |
| Bad dt/NaN/Inf, oversize, empty/unsupported shapes, insufficient budget, cancellation/deadline, arithmetic overflow | Explicit refusals before plans/work buffers; output overflow refused |
| Focused + legacy regressions | Full analyze lib: **22 PASS**, one benchmark intentionally ignored; includes existing coupled charge/energy, shared budget/deadline and authored resonant tests |

One initial compile failed on test-only serde generic inference; repaired with explicit `Value`, original compile log retained. Final compile PASS. No numerical failure, gate adjustment or benchmark repeat occurred. No addon rebuild, TS check or PCB run was required for this internal-only module; the task explicitly excludes an addon rebuild just for this primitive.

## One bounded benchmark

Debug scalar CPU/F64, timing includes plan/scratch allocation. N=131072, kernel length 257, Fs=1024 Hz, finite 128 s record. The long convolution compares a 512-output prefix against independent direct sums; a separate 512-point equal-length FFT/DFT comparison checks Fourier output. This is not a speed claim about the SI pipeline.

| Metric | Result |
|---|---:|
| Long FFT | 0.08245 s |
| Long linear convolution | 0.49868 s |
| Independent direct 512-output prefix | 0.00366 s |
| Same-length 512 DFT / FFT | 0.01475 / 0.000247 s |
| Roundtrip max absolute error | 1.61003e-15 |
| Direct convolution prefix max error | 9.13518e-16 (gate 1e-12 + 2e-11 × 109202.1769) |
| Independent 512 DFT max error | 3.70722e-11 (gate 1e-12 + 2e-11 × 400.3575) |
| PSD integral / independent weighted mean square | 0.925 / 0.9249999999999958 V² |
| PSD integral error | 4.10783e-15 V² (gate 1e-24 + 5e-11 × expected) |
| Conservative convolution reservation incl. borrowed inputs/plans/scratch | 119672848 bytes |
| Conservative spectrum reservation | 35192872 bytes |
| Hard Windows job elapsed / peak aggregate commit | **1.46304 s / 78479360 bytes (74.84 MiB)** |
| Hard caps / exit | 60 s CPU/wall, 512 MiB process/aggregate / 0 |

One numerical test executable was launched. Windows job accounting records three total process entries including supervision; it is not a count of three numerical workers. No process-tree identity capture was added. Compile lies outside the benchmark job. Operation reservations are conservative constructor peaks; retained results and JSON/allocator/runtime storage also fit the measured aggregate cap. Memory plans are per operation, so future callers must reserve retained storage from their remaining budget.

The saved-array Python audit independently recomputes direct convolution/DFT, PSD integral, all per-bin amplitude/PSD factors, Hz shape and frozen hashes, without an FFT package: PASS. `time.png` and `spectrum.png` were opened and visually inspected; axes distinguish V, V²/Hz, seconds and Hz, and annotate DC/off-bin behavior. Figures and raw arrays are in ignored `debugging/p6-signal-kernel/`.

## Package and limitations

`debugging/p6-signal-kernel/manifest.json` hashes the new source, fixture/report/task, both locks, crate/source audit, compile/test/benchmark logs, full long input/output/spectrum arrays, resource measurements and both PNGs. The accepted Windows supervisor is reused unchanged. `dependency.json` captures pinned RustFFT license/features/archive and audited plan-source hashes; common Cargo changes were coordinated with Thermal/P7. RustFFT default SIMD features are disabled and no external runtime is introduced.

Cancellation is cooperative around synchronous RustFFT calls and periodic scalar loops; an individual call cannot observe cancellation mid-call. Existing disposable analysis workers provide hard process termination, but this internal API has no new DSL/worker wiring. Max length 262144 and memory/deadline admission bound this primitive; there is no automatic unknown-band continuation, nonuniform resampling, arbitrary S-table causality, universal resonant fitting or user-facing spectrum API.
