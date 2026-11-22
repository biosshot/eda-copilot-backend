# Authored resonant Y runtime verification

Runtime support is deliberately restricted to `passive-resonant-y`: one or two
ports, one positive series-RLC section per authored constant real orthonormal
mode, explicit DC slope and high-frequency inverse-inductance closure. This is
an assertion by the resource author, not recognition of an unknown sampled
class. It does not close P6, P11, generic SI/PI or PCB extraction.

The exact object/units were recorded first in
`electrical-core-completion-semantics.md`. `resonant.rs` performs conditional
structural identification, with alternating training/holdout rows, three-column
column-scaled reorthogonalized QR (no normal equations), positive dimensional
R/L/C and k/a/b, closure, reciprocal/modal/full-matrix residual, finite ordered
DC/positive samples and linewidth resolution checks. No general vector fit,
extra poles, pole reflection, clipping or noisy-data repair enters production.

The descriptor is exactly A=[U;-I], B=[I;0], internal diagonal C and branch
diagonal positive R/L. The existing coupled AC solver and adaptive backward
Euler transient accept this structure without changing any physical rule.
Neither the integrator nor the two production energy guards were modified.
Steady initialization and state energy remain assembled physical quantities.
Fitted resources retain finite-band conditional provenance; SI voltage data and
its derived plots retain band/hash applicability. Ordinary frequency results
remain samples and do not gain an inferred temporal contract. Existing
sampled-RC admission remains on its original separate path.

Frozen series/mutual fixtures were reused unchanged. Native descriptor AC is
compared with a separate full physical-impedance inversion; time currents and
storage are compared with closed-form delayed ramps. Runtime gates and scales
are recorded in `tests/fixtures/analysis-resonant-runtime/acceptance.json` and
were already used by the first focused tests. The production BE time steps are
adaptive; maxStepS is a cap, not a copied RK4 integration rule. The accepted
checkpoint's coarse RK4 energy failures remain intact as historical evidence.

Unsupported: parallel-Z ideal inductor export, S/Touchstone temporal conversion,
arbitrary or frequency-dependent U, universal VF, inferred delays, unspecified
closure, unknown extra resonances, noisy-data policy. The accepted hidden-Q=1e7
fixture remains a proof that a finite holdout cannot exclude an unseen feature.
Block-diagonal composition does not extract absent intermodel mutual C/L.

## Reproduction and evidence contract

The initial metadata is constructed by
`tests/helpers/electrical-resonant-model.ts` from the accepted immutable fixture
files. The helper verifies the accepted manifest SHA256 and input bytes before
each use. The new scenario does not regenerate AC samples or tune references.
Its separate physical matrix inverse and closed-form ramp states are test-only.
The historical checkpoint scripts/results and all earlier ignored evidence are
preserved.

Native focused tests include dimensional R/L/C recovery and closure, every
matrix entry through the existing descriptor AC solve, actual series/mutual
production transients and independent time refinement, biased assembled DC
capacitor energy, malformed metadata/activity/conditioning/resolution/source-band
and budget refusals. Existing physical/coupled/mixed/relaxation regressions are
run separately with no name filter. The initial filtered invocation's zero
integration counts are retained in the focused log and are not counted as
regressions.

The direct mode calls the source scenario kernel with its real production
executor. Compiled mode calls the built package API and disposable compiled
worker. Installed mode packs/installs into an isolated consumer and calls that
package's public worker API. Each uses the same frozen standalone two-port
scenario, two adaptive transient solves and independent-frequency AC samples.
Ten negative source/metadata substitutions must reject; saved-field corruptions
must fail the independent audit. Voltage and its abs() derivative must retain
conditional band and source hashes. The derived AC Resource must remain
frequency-only without an invented temporal contract.

After the normal jobs=1 addon build and package typecheck/build:

```powershell
python tests/fixtures/analysis-resonant-runtime/bounded.py debugging/p6-resonant-runtime/direct-supervisor.json node --import tsx tests/helpers/electrical-resonant-runtime.ts direct
python tests/fixtures/analysis-resonant-runtime/bounded.py debugging/p6-resonant-runtime/compiled-supervisor.json node --import tsx tests/helpers/electrical-resonant-runtime.ts compiled
python tests/fixtures/analysis-resonant-runtime/bounded.py debugging/p6-resonant-runtime/installed-supervisor.json node --import tsx tests/helpers/electrical-resonant-runtime.ts installed
node --import tsx --test --test-concurrency=1 tests/electrical-resonant.test.ts
python tests/helpers/electrical-resonant-render.py
python tests/fixtures/analysis-resonant-runtime/finalize.py
```

Only `run`-like public scenarios need the coordinated numerical slot. Saved
audits can be repeated with the runner's `--saved` flag, without executing native
calculations again. The Windows supervisor has no unbounded fallback: hard
512 MiB process and aggregate-job commit, 60 s CPU and wall, kill-on-close;
workers inherit the job. Python/Node launchers are auxiliary; numerical solves
remain serial in one process. Each native request also retains its original
256 MiB/60 s budget. Compilation is outside the coupon job and uses the isolated
P6 target. No permanent resource-limit increase is made.

## Results — 2026-10-24

PASS within the declared class. Native: resonant **5/5**, coupled **9/9**,
physical **8/8**, mixed **13/13**, relaxation **6/6** (41 total). New/existing
TS adapter, saved-audit and Resource propagation checks: **8/8**, no skips.
Typecheck and package build passed. One normal default-GPU/static-CRT addon
build, jobs=1, took 475.395 s; no second addon build was needed.

Direct source execution, compiled public worker and installed package each
completed three positive calculations and ten requested negative substitutions.
All numerical arrays and quality values match exactly across the three modes;
the installed archive has exactly one addon, byte-identical to the tested addon.
Placement contract `f32-rte-ftz-v1` passed in the runner and installed consumer.

| Mutual two-port measurement | Observed | Gate |
|---|---:|---:|
| Structural fit, full-matrix absolute normalized error | 1.523e-14 | 0.5% relative +1e-8 absolute |
| Public AC independent-frequency maximum normalized error | 0.226443% | 0.5% |
| Coarse production current error / .02 A scale | 0.028747% | 1% |
| Fine production current error / .02 A scale | 0.014378% | 1% |
| Independent time refinement / .02 A scale | 0.014369% | 0.1% |
| Fine voltage error | 71.891 microvolt | 5 millivolt |
| Fine state energy reference error | 4.790e-13 J | 2e-11 J +1e-20 J |
| Fine relative linear residual | 2.243e-16 | existing 1e-8 guard |

Ordinary AC retains complex interpolation of original samples, which accounts
for its larger error than structural descriptor AC. Native series and mutual
tests independently evaluate exported descriptors at separate frequencies; they
do not validate the export by calling the fit predictor itself.

Fine final energy ledger: work 9.9227821021e-10 J, physical R loss
4.1184632259e-10 J, storage change 5.7995198474e-10 J. Raw work-loss-storage is
4.7990287640e-13 J; separate BE numerical dissipation is 4.7990287628e-13 J;
the existing guarded defect is 1.2366743957e-22 J, normalized 6.23149e-14.
Both original production energy bounds and nonnegative loss checks pass.
Raw balance is not substituted for the guarded defect or hidden by a changed
floor. The frozen RK4 coarse FAIL remains a different historical experiment;
this implementation uses the already accepted production BE ledger unchanged.
The additional runtime analytic storage gate is explicitly 1% of 2 nJ, alongside
the unchanged production guards, not the prototype RK4 quadrature gate.

Adaptive BE accepted 16,311/32,661 steps in coarse/fine runs, with respective
2 ns/1 ns maximum accepted step caps. Output has 2,001 rows at 10 ns cadence.
The integrator chose smaller steps where needed; output sampling did not choose
integration steps. Bias initialization independently retains the expected
nonzero capacitor energy and zero DC branch current.

Negative outcomes include orthogonality, absent/wrong closure dimensions,
nonfinite JSON, active samples, conditioning (bound 4.135e12), insufficient
linewidth sampling, source band, unsupported Z and unknown class. Ten saved
field corruptions (currents/voltages/energy/guard/units/band/hash/plot validity
and plot values) are independently rejected. The abs() derivative retains
conditional band/hash provenance. A direct executor test confirms that AC
derived Resources retain frequency-only permissions and no temporal metadata.

One pre-existing Resource propagation assertion still expected all coupled
composition to reject. It was corrected to accept the previously approved
coupled cascade while retaining the strict direct reordered-port refusal; no
frequency execution or physical rule was changed for this correction.

| Bounded job | Wall s | User CPU s | Kernel CPU s | Peak aggregate commit MiB |
|---|---:|---:|---:|---:|
| Direct | 2.581 | 2.719 | .500 | 183.023 |
| Compiled worker | 3.963 | 3.109 | .953 | 279.246 |
| Installed (including local pack/install) | 20.409 | 7.141 | 5.469 | 339.781 |

All jobs stayed below unchanged 512 MiB/60 s hard limits. Numerical work was
serial; build/worker slots were coordinated with P7/Thermal. No PCB solves,
new agents, dependency changes or changes to other worktrees were made.

Addon SHA256:
`9a5c2363203a78975eabd484262f55f1730ed212cea3d4e3b136560ab02f8cc5`.
Installed archive SHA256:
`d1246e83fb35515b3b9e4ed863b698e733b4b352cc5e8f216f6e156fed9ba337`.
Original frozen manifest remains
`342a2af63f6dc2140c987e6a56d6955178b41cfb30a824aa88cd2a3ae58796a2`.

Saved `debugging/p6-resonant-runtime/{metrics,manifest}.json` contain hashes,
raw-number audits, exact mode parity and measured resource data. Raw results,
scenario bytes, references, wrapper metadata and logs are retained in each mode.
Three static direct-mode figures (`ac-all-entries.png`, `transient.png`,
`energy.png`) were generated only from saved arrays and visually inspected:
all complex matrix entries, both port voltages/currents, storage error and the
separate raw/BE/guarded ledger are visible without clipped labels.

The first direct run's numbers were valid, but its audit incorrectly read a
public Network handle as a raw Resource. The audit was fixed, the first arrays
were checked read-only and preserved in `direct-before-harness-fix`; one clean
direct replay after the harness fix has exactly identical numerical arrays.
Supervisor-only flag/stdout fixes happened before the clean jobs; neither raised
limits. `journal.md` records these failures and the stale-test corrections.
No native calculation was repeated for plotting, final hashes or saved audits.

Ready for parent review, uncommitted. The supported subset is complete for this
task; general sampled resonant identification, parallel ideal-L export and full
P6/P11 closure remain outside it.
