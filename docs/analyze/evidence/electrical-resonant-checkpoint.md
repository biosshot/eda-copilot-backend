# P6 resonant numerical checkpoint

Scope: isolated CPU/F64 prototype; no production fitter, API, addon, dependency,
placement or solver change. This does not close P6 or P11. Frozen inputs and
executable experiment are in `tests/fixtures/analysis-resonant-checkpoint/`.
Saved numerical evidence is under ignored `debugging/p6-resonant-checkpoint/`.

## Recommended first class

Admit only an authored reciprocal admittance with a constant real orthonormal
modal transform U, initially one or two ports and one damped series-RLC section
per mode:

`Y(s) = U diag(k*s/(s*s+a*s+b)) Uᵀ`, with `k,a,b > 0`.

Then `L=1/k`, `R=a/k`, `C=k/b` are positive. The modal poles have negative real
parts and the physical network proves positive realness globally. This is a
restriction on the authored model, not something inferred conclusively from
finite measurements. A constant U is itself an authored restriction; real
reciprocity alone does not imply simultaneous frequency-independent modal
diagonalization. Check symmetry, orthogonality and off-diagonal modal residuals
before any future admission. No delay, general Touchstone conversion,
frequency-dependent U, arbitrary resonator count or data repair is proposed.

The present `relaxation.rs` only realizes nonnegative real-pole RC relaxation
admittances. It cannot represent these complex-pole resonances. Its finite-band
conditional validity, authored closure, bounded fitting, source-front and
refinement requirements remain applicable; this checkpoint does not extend it.

The parallel-RLC impedance coupon tests the dual mathematical class
`Z=U diag(k*s/(s*s+a*s+b)) Uᵀ`, with `C=1/k`, `R=k/a`, `L=k/b`.
An exact ideal parallel inductor has zero series resistance: exporting this
class through the current coupled backend would conflict with its strict
positive-definite R contract. Therefore parallel Z is a numerical coupon, not
part of the first proposed production admission. Do not insert ESR silently.

For the proposed Y class the existing descriptor suffices. Use external physical
port nodes, one internal capacitor node per mode, `Cinternal=diag(C)`,
`Lbranch=diag(L)`, `Rbranch=diag(R)`, incidence
`A=[U; -I]` and physical port injection `B=[I;0]`. This eliminates ideal modal
transformers algebraically, retains R positive definite, and avoids a new runtime
solver. All intermodal coupling is authored by U; block-diagonal assembly cannot
extract absent intermodel mutual C/L. Production integration still needs its own
review, admission tests and resource accounting.

## Same-data comparison and proof boundaries

Both methods receive the same frozen training Resource, authored U and closure;
holdout is separate and never used for initialization or fitting. SI immittance
is normalized by 1/Rnorm for Y and Rnorm for Z; `z=s/(2*pi*1 MHz)`.
The structural fit solves the real linear problem
`1/Hnorm = r0 + r1*z + rMinus1/z` by column-scaled least squares, rejects
nonpositive coefficients or condition number >1e10, and recovers K,A,B.
It does not clip coefficients. Nominal orders 2/4/8 add positive resonant atoms
under an equality-constrained simplex fit (total high-frequency K fixed).
An exhaustive active-face search has at most four atoms. Actual active section
count is reported: recovering a one-section truth does not demonstrate general
multi-resonator identification or effective higher-order convergence.

The rational comparator implements VF sigma pole relocation, using a real
conjugate partial-fraction basis and an eigenvalue relocation step. It starts
from the training peak, never the analytic truth. It rejects unstable poles;
there is no pole reflection. The final order-two residue solve explicitly
enforces authored DC=0 and leading high-frequency coefficient K. Its positive
K,A,B then give the same physical certificate. Higher-order failures or lack of
a certificate are reported as refusals, not hidden by a successful finite scan.
These are restricted prototype algorithms, not a claim to implement the full
Matrix Fitting Toolbox or its passivity enforcement.

[Gustavsen and Semlyen (1999), primary paper](https://people.math.ethz.ch/~hiptmair/Seminars/RAP_22/GUS99.pdf)
and the [official SINTEF VF algorithm](https://www.sintef.no/en/software/vector-fitting/algorithm/)
describe pole relocation; our rejection policy deliberately differs from the
toolbox's pole reflection option. SINTEF documents
[passivity as a separate constraint](https://www.sintef.no/en/software/vector-fitting/passivity/).
Stable poles alone do not imply passivity: negating a positive RLC admittance
preserves poles but makes its Hermitian real part negative. Finite sampled
eigenvalue checks cannot prove global positive realness of unknown data.
The structural constrained fit uses elementary active-face least squares;
the [official NNLS documentation](https://docs.scipy.org/doc/scipy/reference/generated/scipy.optimize.nnls.html)
provides background on nonnegative least squares. No external fitter or circuit
runtime is called; only NumPy linear algebra and this script's integrator run.

## Closure, sources and states

Each authored section has DC H(0)=0, slope k/b, and high-frequency H(s)~k/s.
Those two asymptotic coefficients are explicit sidecar data, checked against
the fit. They are not extrapolated from the highest or lowest sample. Unknown
extra poles, feedthrough, shunt capacitance, propagation delay and out-of-band
behavior remain unknown unless excluded by that authored contract. Missing
closure refuses. A finite source rise >=10/fmax is necessary here, not a proof
that a ramp has compact spectral support.

Ordinary `network-parameters` Resource data, Y/Z representation, named ports,
complex parameter matrices and common passive reference topology carry AC
samples. `checkpointContract` is explicitly a prototype sidecar, not a supported
production `temporal` method. A future resonant method would require an explicit
versioned temporal contract with U, branch count, DC/high-frequency closure,
conditioning, fit/resolution diagnostics and conditional source validity.

Canonical states satisfy `x1'=x2`,
`x2'=u-(a+load*k)*x2-b*x1`, `y=k*x2`.
`E=k*(b*x1²+x2²)/2` and
`E'=u*y-(a/k+load)*y²` prove nonnegative storage and dissipation.
Y uses finite Thevenin source resistance 25 ohm; Z uses a Norton current source
with finite parallel load. The latter is a mathematical boundary condition,
not an assertion that the current SI builder supports current injection.
U rotates input/output power conjugately. Runs start from explicit zero energy
and a ramp from zero. For nonzero DC, assembled equilibrium must determine
`x1=u/b, x2=0` with physical capacitor voltages/inductor currents, not guessed
zero states. Future arbitrary initial states need the existing physical
charge/gauge admission; fitting alone cannot determine initial energy.

The AC oracle directly inverts the physical R+sL+C^-1/s matrix (or parallel
admittance), including all four mutual two-port entries. Time reference is the
closed-form damped sinusoid integrated analytically for each delayed ramp;
the fitted model uses independent RK4 including power/loss quadratures.
Raw `work-loss-E` is reported without a numerical correction term.

## Frozen experiment and gates

Before fitting, `definition.json`, nine training/holdout fixtures and
`frozen-manifest.json` were saved. They specify all SI component values, U,
normalizations, initial states, source/load values, grid sizes, time steps and
gates. Training has 257/1025/2049 positive logarithmic rows over 10 kHz–100 MHz
plus DC; independent holdout uses geometric cell midpoints. Minimum local
linewidth resolution is eight samples. Coarse-grid refusals remain refusals.

All matrix elements use `|error| <= .005*|reference| + 1e-8` in normalized units;
order refinement uses `.001*|reference| + 1e-8`. Time gate is 1% of the predefined
scale (series/mutual .02 A, parallel .1 V) plus 1e-12 SI. Energy reference gate
is 1e-4 of the predefined energy scale plus 1e-20 J; raw balance gate is
1e-6*(|work|+loss+E)+1e-20 J. These gates were not tuned to results.
Orders and grid resolution are varied independently. Time steps 2 ns and 1 ns
are compared on a common 10 ns output grid, through 20 microseconds.

Each numerical coupon runs as one CPU/F64 process with single-thread BLAS,
Windows Job hard process-memory cap 512 MiB, CPU limit 60 s and a 60 s wall
deadline. Supervisor setup failure stops without an unbounded fallback.
Rendering reads saved numerical files and does not repeat fitting/solves.

## Results (2026-10-24)

The restricted, authored class is supported at the **fine 1 ns step**. The
original strict all-time-steps aggregate is **false**, retained in raw results:
series and mutual fail the additional energy gate at 2 ns. This is not an
unconditional package PASS. No gate, fixture or normalization was changed after
fitting. No extra refinement run was added: 1 ns was already frozen in the plan.
The production blocker for generic sampled models remains identification of
unknown intersample/out-of-band resonances; the narrow first class requires the
authored structural assumption. Exact parallel-Z export has the separate R-PD
blocker described above.

Errors below are fractions, not percentages. Matrix numbers use
`max |error|/(|reference|+2e-6)` on independent holdout. Fine time errors are
fractions of predefined output scale.

| Coupon | Structural matrix max (2049) | VF order 2 matrix max | Fine time fraction | VF fine time fraction | Fine raw balance max J |
|---|---:|---:|---:|---:|---:|
| Series Y | 1.369e-14 | 3.925e-15 | 3.001e-11 | 3.001e-11 | 6.834e-20 |
| Parallel Z | 1.493e-14 | 4.064e-15 | 4.684e-10 | 4.684e-10 | 6.944e-22 |
| Mutual Y | 5.649e-14 | 9.986e-15 | 3.652e-11 | 3.652e-11 | 6.903e-20 |

The dense 4097-row independent physical-matrix audit reports each entry:

| Coupon/entry | Normalized error | Absolute normalized error |
|---|---:|---:|
| Series 11 | 1.373e-14 | 1.372e-14 |
| Parallel 11 | 1.545e-14 | 1.537e-14 |
| Mutual 11 | 1.316e-14 | 1.165e-14 |
| Mutual 12 | 5.649e-14 | 5.088e-15 |
| Mutual 21 | 5.478e-14 | 4.959e-15 |
| Mutual 22 | 8.777e-15 | 6.037e-15 |

Structural orders 2/4/8 select exactly one active section per mode, with weight
1. Order changes are zero, satisfying the fixed 0.1% gate. Effective order is
2 per mode throughout (4 total for the mutual coupon). VF order 2 relocates in
two iterations on these exact data; higher orders are correctly refused for
overparameterized rank/conditioning (~1e15–1e16), not claimed converged.
This favors the structural fit for this restricted class: simpler, direct
physical certificate, and no unsupported extra poles. It does not establish
robust identification under noise or superiority for arbitrary rational data.

| Positive training rows | Series samples/linewidth | Parallel | Mutual minimum |
|---|---:|---:|---:|
| 257 | 14.106 PASS | 2.821 REFUSE | 6.141 REFUSE |
| 1025 | 55.673 PASS | 11.135 PASS | 24.676 PASS |
| 2049 | 111.095 PASS | 22.219 PASS | 49.240 PASS |

Exact authored data can yield near-roundoff fits even on the refused coarse
grids; that does not waive the resolution requirement. For admitted grids,
all matrix gates pass. Increasing frequency resolution does not imply a proof
that an unknown narrow feature is absent.

At 2 ns the series/mutual maximum raw balance errors are 1.090e-18/1.096e-18 J;
their worst balance ratios are 9.460e-6/9.193e-6, failing the unchanged 1e-6
gate during the small-energy start. At the planned 1 ns step these ratios are
5.908e-7/5.744e-7 and every point passes the original gate including its absolute
floor. Parallel passes both steps. Output changes between time steps are
4.521e-10, 7.031e-9, 5.486e-10 of scale for series/parallel/mutual.
Fine energy errors against the analytic state energy are
8.036e-20, 5.107e-21, 8.315e-20 J. No signed history or integration correction
was added to energy acceptance.

All negative audits behave as intended:

- Active sample is rejected (minimum normalized Hermitian eigenvalue -0.99986)
  despite unchanged stable poles.
- Missing authored closure and 1 ns source rise (minimum 100 ns) are rejected
  through the same preflight used by positive coupons.
- Clustered training frequencies produce condition 1.679e12 >1e10 and refuse.
- Coarse Q=10 grid has only 2.821 samples/linewidth and refuses.
- A passive hidden branch with Q=1e7 at 1.503232958 MHz passes finite holdout
  (1.668e-5) but misses the independent peak by 0.35395 (35.395%). This is a
  constructive non-identifiability counterexample. The script labels it
  uncertified; no finite-grid admission rule claims to detect every such branch.

## Evidence and reproduction

Python 3.11.0, NumPy 1.26.4, Windows 10.0.26200, one BLAS thread. Times include
numeric work and output, but not interpreter/import startup; the hard timer and
job limits are installed before NumPy import. All measured memory peaks and
hard limits are in each raw JSON.

| Process | Wall s | CPU s | Peak working set MiB | Peak commit MiB |
|---|---:|---:|---:|---:|
| Series | 7.879 | 7.797 | 38.949 | 60.461 |
| Parallel | 7.554 | 7.531 | 39.027 | 60.313 |
| Mutual | 7.805 | 7.734 | 44.730 | 66.945 |
| Negatives | .027 | .031 | 36.348 | 58.481 |

Frozen manifest SHA256:
`342a2af63f6dc2140c987e6a56d6955178b41cfb30a824aa88cd2a3ae58796a2`.
`debugging/p6-resonant-checkpoint/evidence-manifest.json` records individual
SHA256/size for inputs, scripts, raw results, traces, figures and metrics.
`metrics.json` is a read-only audit of saved results, retaining both
`fineCheckpointPass=true` and `strictAllStepsPass=false`.

Run from the worktree root; these commands require a coordinated heavy slot
for `run`. They deliberately return exit 1 on the two strict aggregate failures.
Do not regenerate the frozen fixtures when reproducing the saved experiment.

```powershell
python tests/fixtures/analysis-resonant-checkpoint/checkpoint.py run series
python tests/fixtures/analysis-resonant-checkpoint/checkpoint.py run parallel
python tests/fixtures/analysis-resonant-checkpoint/checkpoint.py run mutual
python tests/fixtures/analysis-resonant-checkpoint/checkpoint.py run negative
python tests/fixtures/analysis-resonant-checkpoint/checkpoint.py plot
python tests/fixtures/analysis-resonant-checkpoint/audit.py
```

`prepare` is solely the deterministic initial fixture-generation command.
The failed first negative-report serialization (NumPy boolean) was fixed and
that tiny negative process alone repeated. Positive numerical calculations were
not repeated. Six saved figures (`series/parallel/mutual-ac.png` and
`*-time-energy.png`) were visually inspected in `qa-contact-sheet.png`: all
matrix entries, analytic/fitted transients and raw energy residual are visible.
`journal.md` preserves run/refusal/serialization details. No native build or PCB
solve was needed; no unrelated earlier package was rechecked.

Next implementation scope, only after review: explicit one-section-per-mode
admittance admission, structural fitting and existing coupled-descriptor export,
with authored U/closure, current resource budgets and source validity checks.
No general vector fitter, parallel ideal-L export, arbitrary order escalation,
noisy-data policy or complete P6 closure is authorized by this experiment.
