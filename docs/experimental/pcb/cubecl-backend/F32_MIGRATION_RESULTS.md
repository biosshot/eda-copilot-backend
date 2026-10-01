# F32 migration: results and closure

Date: 2026-10-01. Branch: `feat/pcb-f32-migration`.
**Roadmap closed at the user's instruction after scope reduction and cleanup.**
The final artifact and actual checks are recorded under **Cleanup verification**.
Historical intermediate results below retain their original evidence scope;
open-at-the-time statements are not new tasks. Coordinate-frame restrictions
and recorded ESPower route-quality limitations remain unchanged for later work.
Closure does not assert completion of every original benchmark or a 2× speedup.
See [the closed roadmap](F32_MIGRATION_ROADMAP.md).

## Baseline and artifacts

The frozen F64 anchor is `51274d56778cb03f8b3fda18c5a22e0ecdc6bd64`.
Its separate addon is 20,088,320 bytes, SHA-256
`4bf83e7e051fe83c0cd38b2c6b924885c3927c9811eaaec1159e2fc1a266799c`.
Baseline sources, hardware/driver, flags and saved-reference inventory are under
`debugging/f32-migration-2026-10-01/baseline/`. The old addon is not a production fallback.

| Local artifact | Bytes | SHA-256 | Evidence scope |
|---|---:|---|---|
| v2 Windows addon | 20,175,360 | `076f5ea7422554b61575ccf561423c038a349d19858e0a4598aee4c37c9e5636` | Native API, GPU board/refiner/recovery checks; four test failures recorded below |
| v3 Windows addon | 20,296,704 | `34da5fcdd4f7b045e647f396c4786582c79bf52e556e5104f22caf5299cd79b0` | Canonical captures, strict shader audit and relation-expression enclosure; targeted verification still found a final-score diagnostic failure |
| v4 Windows addon | 20,321,792 | `247c64d343cbbc65a91bfefce14dc78aab92a404b9486588afbb66ee46aabcfb` | 43 actual-GPU checks passed; one quantized path diagnostic failed |
| v5 Windows addon | 20,322,304 | `9f29da6e13ead809b05763f97f131bd71eb354847ddec93a154ac4843ba6b3f6` | Failed combined-term case now passes both compactness modes; four full block cycles below |
| v6 Windows addon | 20,322,816 | `60ba91cf340a1ec8f6719ccdd9f4e6d860d65e0706de6ae2812b09dd3e62d539` | Exact-prefix pruning separated from cache uncertainty; redundant FTZ copies removed; all 44 GPU tests pass |
| v7 Windows addon | 20,334,592 | `e33394664dda14e59799976bc51d8d7f6158a3d7f87db97425833979fce5b1e6` | Unresolved-route/score-addition bounds; complete five-block package and Telemetry full pipeline |

Each immutable version lives under the ignored `debugging/f32-migration-2026-10-01/`.
The v3/v4/v5/v6/v7 manifests record source hashes and release command. Later source changes do
not retroactively belong to that artifact. Windows builds use `npm run native:build`,
locked Cargo dependencies, release optimization and static CRT. No second production
precision, helper DLL or CPU JIT was introduced. Linux/macOS packaged addons have
not been rebuilt on this host; the updated loader rejects their old numeric contract.

## Agreed arithmetic and observed GPU

The user explicitly accepted signed FTZ on 2026-10-01: subnormal F32 inputs/results
become signed zero. CPU calls and solver workers use a scoped RTE/FTZ/DAZ guard,
restoring the entering thread's state on return/unwind. TypeScript narrows operands
and individual results through the shared helpers. Integer IDs/counts/angles remain
integer; coordinate transport and timers are documented binary64 exceptions.

The tested NVIDIA GeForce RTX 3060 Laptop GPU Vulkan driver reports F32 RTE and
SignedZeroInfNanPreserve support, but both DenormPreserve and DenormFlushToZero
properties are false. Therefore the current shader pass explicitly canonicalizes
subnormal float loads/results using integer bits. It requests RTE and preservation
of special values and decorates arithmetic with NoContraction. F64/fast-math/relaxed
precision/explicit FMA are rejected by the shader pass. A native F64 capability is
not required; hardware without F64 has not separately been tested.

Actual FTZ/RTE/no-contraction probes passed in `runtime-gpu-ftz-explicit-1.log`.
Earlier preserve and selectable-FTZ admission failures are retained as evidence;
they were superseded by source/contract changes, not rerun for a better time.
The performance cost of explicit shader canonicalization remains unmeasured.

Vulkan division permits 2.5 ULP and Sqrt inherits InverseSqrt/division precision;
RTE does not strengthen those API guarantees. See the [Vulkan precision table](https://docs.vulkan.org/spec/latest/appendices/spirvenv.html#spirvenv-precision-operation).
A recorded board relation term was 234324.078125 CPU versus 234323.96875 GPU.
Its quadratic distance-limit penalty amplifies the primitive difference. The v3
outward F32 enclosure covers 234322.546875..234325.53125 on identical materialized
F32 geometry. This is a scorer diagnostic; it proves nothing about original
physical geometry and changes neither clearance nor candidate acceptance.

## Verification ledger

| Command/check | Result | Evidence |
|---|---|---|
| Rust default-feature unit tests after interval/capture changes | 73 passed, 2 ignored | `rust-gpu-unit-6.log` |
| Rust unit tests after route-bound changes/shared F32 distance | 74 passed, 2 ignored | `rust-gpu-unit-8.log` |
| Rust CPU-only tests | 70 passed, 1 ignored | `rust-cpu-unit-4.log` |
| TypeScript typecheck through canonical-capture/test changes | Passed | `typecheck-f32-5.log` |
| Five native/API/rounding/route files, v2 | 43 passed, 6 GPU checks skipped | `ts-native-tests-2.log` |
| Actual board/refiner GPU suites, v2 | 40 passed, 4 failed | `ts-gpu-tests-v2-1.log` |
| Six targeted GPU checks, v3 | 5 passed, 1 failed | `ts-gpu-tests-v3-targeted.log` |
| Actual board/refiner GPU suites, v4 | 43 passed, 1 failed | `ts-gpu-tests-v4-1.log` |
| Failed combined endpoint/topology/net/alignment case, v5 | Passed normal and high compactness; actual GPU | `ts-gpu-tests-v5-targeted.log` |
| Rust default-feature units, v5 | 74 passed, 3 ignored | `rust-gpu-unit-10.log` |
| Six native/API/route files, v4 | 48 passed, 6 GPU checks skipped | `ts-native-tests-v4-1.log` |
| Typecheck and package build, v5 | Passed | `typecheck-f32-7.log`, `package-build-f32-1.log` |
| Emitted shader binary structural audit | 31 v4 and 12 v5 kernels passed | `v4/mass-shaders-audit.json`, `v5/mass-shaders-audit.json` |
| Rust units after exact-prefix pruning and FTZ copy analysis, v6 | 76 passed, 3 ignored | `rust-gpu-unit-12.log` |
| CPU-only Rust after exact-prefix pruning, v6 | 71 passed, 1 ignored | `rust-cpu-unit-5.log` |
| Complete actual board/refiner GPU suites, v6 | 44 passed, no skips | `ts-gpu-tests-v6-1.log` |
| Emitted v6 shader structural audit | 31 kernels passed | `v6/mass-shaders-audit.json` |

The native/API files are `pcb-f32-numerics`, `pcb-board-packer-native`,
`pcb-block-search-stages`, `pcb-post-place-refiner`, `pcb-route-cost-comparison`,
and, for the six-file pass, `pcb-pad-crossings`.
The GPU suites are `pcb-board-gpu` and `pcb-post-place-refiner`; flags explicitly
enable GPU checks. Successful tests require actual GPU backend logs, rather than
accepting a CPU replay as a GPU result. They cover batch/stage failure, fresh CPU
timeouts, scratch/lease release, chunk boundaries, worker budgets and mixed consumers.

Three v2 failures were stale expectations: an F64 no-device message and two
out-of-frame inputs. Tests now check explicit boundary rejection, retaining the
wider-frame limitation; passing those tests does not accept broader input support.
The remaining v2 failure was the relation term above. V3's enclosure resolved that
term check but exposed amplification at the final score (3108.047607421875 versus
3108.0419921875). Source now recomposes the already checked GPU terms using CPU
weights/order; v4 verified this recomposition. Its remaining path-term case was
510.2336120605469 CPU versus 510.234619140625 GPU: adjacent 0.001-grid values
represented in F32 differ by 0.001007080078125. V5 permits the existing one-quantum
diagnostic allowance plus its F32 representation error (outward rounded).
This applies only to that path diagnostic, never hard constraints or min_delta.
The failed release test was rerun after this change and passed both compactness
modes. The other 43 unchanged passing GPU cases were not repeated.

The shader audit reads actual assembled words, checks F32 types, supported modes,
NoContraction on arithmetic, absence of relaxed/fast math/FMA/subnormal constants,
and four-byte decorated scalar float arrays. Private/function arrays legitimately
lack interface ArrayStride. This is a structural audit, not an external SPIR-V
validator; no `spirv-val` installation was available on this host.

## Single-pass full block cycles, v5

Unchanged captured PortableScope blocks `00079` (six capacitors), `00097` (eight),
`00100` (six), and `00091` (19-primitive FPGA) use the same worker budget (one).
Each smaller block completes Beam, singles and deferred pairs; FPGA completes
Beam/singles and skips pairs under its existing size rule. CPU and GPU each ran
once, sequentially in separate processes, without verification rescoring.
Commands used `scripts/experiment-block-cubecl.mjs backend=cpu|cubecl
blocks=00079,00097,00100,00091 runs=1 workers=1`, the immutable v5 addon/manifest,
and fresh `v5/blocks-cpu-w1` / `v5/blocks-gpu-w1` output directories.

| Measurement | CPU F32 | GPU F32 |
|---|---:|---:|
| Complete four-block package wall | 106.871 s | 19.639 s |
| Initial calls (Beam/singles) | 104.683 s | 18.656 s |
| Deferred pair calls | 2.189 s | 0.982 s |
| Actual backend | CPU | CubeCL, one initialization, no fallback |

The practical CPU/GPU ratio is 5.44x for this package. All final outputs and
checkpoints match exactly between these two F32 passes. The FPGA GPU search
evaluates 1,596,445 score candidates in 665 batches, plus frontier work; the saved
F64 run evaluated 1,745,768 candidates. This is a changed search trajectory, so
times against the historical F64 package do not measure identical work or prove
a general F32 multiplier. The GPU workspace peak reported here is 1,086,728 bytes;
it excludes total VRAM and process/private memory, which remain unmeasured.

External binary64 reconstruction runs in a separate Node process without addon
or production imports (`debugging/f32-migration-2026-10-01/measure-blocks.mjs`).
It uses original component body/hole boxes at returned raw poses, with no output
grid rounding. The same instrument measures saved F64 and new F32 outputs.
Inventory, locked poses and allowed rotations pass on all four blocks; no raw
pair-clearance violations occur. SVGs and complete identities are saved under
each result's `physical/` directory. Outline, regions, routes and the complete old
validator are outside this instrument's scope; it does not close full acceptance.

| FPGA soft measure | Saved F64 | CPU/GPU F32 |
|---|---:|---:|
| Own solver score (different arithmetic) | 5528.140155 | 5529.883789 (+0.0315%) |
| External HPWL, mm | 87.940000 | 87.519000 (-0.479%) |
| External body envelope, mm² | 479.664970 | 479.289240 (-0.0783%) |
| Minimum margin beyond required pair clearance, mm | 0.001000 | 0.00100010 |

These four cases support comparable block quality. The required small USB case,
local/final refinement and complete pipeline matrix remain open.

## Complete ESPower pipeline, v5

The unchanged 53-component input SHA-256 is
`82f28407129cb28f42f815ab3c14defe7b324c36f523271172caf245cbcc1f86`.
Each complete TS pipeline ran once with one worker, no subtree workers, native
capture and the same v5 artifact. CPU requested all CPU backends; the second run
forced block, board and refiner CubeCL where supported. Both finish every stage,
produce identical final poses, and report `placementOk=true`. Board GPU and all
19 local/final refiner calls use one CubeCL runtime; 18 unsupported block calls
fall back to CPU F32. Those fallbacks are part of the measured pipeline.

| Scope | CPU F32 | Forced GPU F32 |
|---|---:|---:|
| Complete TS pipeline wall | 57.990 s | 60.179 s |
| Captured block batch wall, divided by batch size and summed | 43.331 s | 41.307 s |
| Ordinary board native call | 11.154 s | 14.493 s |
| All local/final refine native calls | 1.572 s | 2.523 s |

Batch sums describe those recorded calls, not additional wall measurements or
CPU worker totals. Forced GPU is 3.77% slower here; no full-board speedup is
claimed. The saved historical F64 pipeline was 46.532 s with block auto and
forced board/refiner GPU, so its backend/input trajectory differs. It made 37
block and 18 refine calls versus 38/19 in v5. New F32 block work consumes more
time; the cache-uncertainty margin unnecessarily disabled even exact-prefix
pruning. Source v6 corrects that separation; its measurements are recorded below.

The unchanged F64 validator/scorer from the frozen anchor was extracted into
`baseline/f64-validator-source/` and runs in separate Node processes against the
separate frozen F64 addon. It accepts saved F64 and both new F32 outputs on the
original input: zero reported overlap, outline, hole, region, layer or inventory
violations. No physical tolerance or fixture changed. External HPWL is 422.100250
mm saved F64 versus 429.430151 mm F32 (+1.74%); this crosses the roadmap's review
threshold and remains a quality review item, rather than being hidden by a
similar own score. Tiny negative raw pair margins are retained by identity:
minimum -0.000100000 mm F64 and -0.000100877 mm F32, within the unchanged existing
TS domain tolerance. Full reports live in `external-f64*/measurement.json`.

The first CPU external measurement had a harness-only HPWL/margin extraction
error; `external-f64-v2/` corrects it. No solver was repeated for that repair.
Original validator files were not edited. Comparison previews
`v5/ESPower-comparison.png` and `v5/FPGA-comparison.png` were inspected: FPGA
retains the compact structure; ESPower chooses a shifted MCU group and a longer
USB path span, consistent with the HPWL review above. Preview inspection is not
a route-feasibility proof.

Windows `llvm-readobj --coff-imports` confirms a single x86-64 addon with OS DLL
imports only (`v5/addon-imports.log`); it has no compiler/helper DLL dependency.
A mid-run OS snapshot records GPU-pipeline child peak working set 394,829,824
bytes and then-current private bytes 458,211,328. This is not the final private
peak or complete process-tree/VRAM measurement; no CPU memory ratio is claimed.

## Subsequent v6 full-pipeline checks

After the recorded source changes, ESPower was repeated once per backend:
53.934 s CPU / 58.624 s forced GPU, both complete and valid. Final poses and
external F64 measurements are unchanged from v5. The exact-prefix fix reduces
some unnecessary block work but does not eliminate the remaining regression
against the historical F64 trajectory. Auto tuning and remaining pruning/quality
analysis are still open. No performance threshold was changed merely to improve
this comparison.

The first full v6 esp32c3 passes take 7.972 s CPU / 10.923 s forced GPU, with
identical final poses and `placementOk=true`. The unchanged external F64 validator
also accepts both. External HPWL is 562.187801 mm versus saved F64 562.457300 mm
(-0.0479%). Minimum measured clearance margin is +0.0000000341 mm; this nominal
positive margin does not itself fulfill the propagated-uncertainty proof in
roadmap 3.2. The default small-input CPU policy is still appropriate; forcing
tiny board/refiner calls onto GPU is not a general performance improvement.

V6 skips redundant shader FTZ checks only for integer-to-float conversion
(result zero or magnitude >= 1), negation and selection of already canonical
float operands. Loads and operations that can create tiny magnitudes remain
canonicalized. Negation retains NoContraction. Structural units, actual runtime
probes and all consumer/recovery tests above validate the changed pass.

V3 captures actual typed local DTOs, frame origins, original poses and locked poses
when `PCB_F32_NATIVE_CAPTURE_DIR` is set. Files are created without overwriting
earlier evidence, before the timed solver starts. This supplements the existing
pre-native TypeScript captures. Actual emitted SPIR-V is saved when
`PCB_F32_SHADER_AUDIT_DIR` is set. Neither capture changes scores or cache keys.

## Telemetry complete ordinary/aligned native pair, v6

The saved native inputs are 00089-board-0daab28e3d (ordinary) and
00090-board-6393e1d4b3 (aligned), unchanged from the historical Board captures.
One sequential CPU/GPU replay uses four host workers, the saved CPU budget.
The complete pair takes 272.131 s CPU / 120.824 s GPU (2.252x). Both F32
solutions match exactly. GPU logs show one runtime, no fallback, and 19,219,480
tracked workspace bytes. The saved F64 CPU pair took 341.976 s; search trajectories
and precision differ, so this is a practical comparison rather than identical work.

Both old F64 and new F32 solutions retain native hardCount=3 and approximately
5.7 mm severity. These are pre-existing native constraint debts, not zero-hard
placements; classification by identity remains open. The unchanged external F64
physical validator accepts both F32 CPU outputs with no reported collisions,
outline/hole/region/layer/inventory violations. Ordinary/aligned external HPWL is
2487.073 / 2580.658 mm, versus old 2516.326 / 2530.414 mm. The aligned branch
worsens by 1.99% and is a review item; F32 selects ordinary, whose HPWL improves
1.71% against the selected old aligned result. Minimum raw pair margin remains
about -0.0006 mm within the unchanged existing domain tolerance.

## Telemetry final-refiner timeout regression, v6

The unchanged saved Telemetry-cpu problem uses 154 components, five iterations,
one worker and the existing 30,000 ms budget. CPU/GPU each ran once, taking
30.081 / 30.101 s with valid returned placements. Both stop at timeout during
the fifth pass; their partial timed outputs differ. These are timeout-quality
evidence, not completed-search acceptance. The saved F64 CPU/GPU calls completed
in 17.634 / 12.887 s.

The v6 conservative route bound disables pruning whenever a baseline route is
unresolved. This causes additional complete route evaluations and a material
regression. Source v7 restores a finite bound using the existing expansion limits
only to bound route cost, includes separate F32 term/sum errors and the final
score-plus-penalty additions, and keeps all routing/search budgets unchanged.
V7 Rust units pass (77/0/3 ignored). The complete-search results follow.

## V7 complete block and refiner checks

The previous four captures plus the required small USB
00003-usb_input-f67ddfba6b complete Beam, singles and all applicable deferred
pairs in 97.951 s CPU / 17.670 s GPU (5.543x), one worker and one pass each.
CPU/GPU outputs and checkpoints match exactly, and external raw-geometry
measurement accepts all five: inventory/locked/rotation checks pass and no pair
clearance violations occur. USB minimum margin is 0.0264 mm and HPWL 15.999996 mm.
The four earlier block poses/measurements are unchanged. Repetition follows the
exact-prefix pruning/FTZ-copy code changes, rather than a repeated warm benchmark.

Telemetry final refiner now completes all five iterations in 14.689 s CPU /
14.709 s GPU, valid and without a timeout. Its route ceiling uses the existing
expansion limits to bound maximum returned cost, per-term/sum errors, and both
score-plus-penalty additions; it preserves all candidates outside a proved bound.
Both backends choose the same five moves and final poses. At scores around
15,858,754, a GPU score differs by one F32 unit and an improvement by one unit;
this is within the arithmetic diagnostic allowance. The old exact-output harness
exits with a mismatch even though validation and placements pass; the recorded
run is retained and reviewed, not rerun. Its new explicit quality mode records
such mismatches for independent review while retaining default exact mode.
Both raw outputs pass the isolated old F64 validator, external HPWL 2498.866201 mm.
The saved F64 CPU completed in 17.634 s. No GPU speedup is claimed for v7's near
equal wall times.

## V7 heavy complete Telemetry pipeline

The chosen heavy input completes all TS/native stages in 510.271 s, forced GPU
where supported, one board host thread, one block worker and one final-refiner
thread. The earlier report incorrectly said four board threads: saved settings
and native logs both show one. The harness defaults `workers` to one and overrides
the inherited environment. It reports placementOk=true; the unchanged external F64 physical
validator also accepts final raw poses with no reported hard physical violations.
External HPWL is 2860.231464 mm and minimum pair margin -0.0005999914 mm under
the unchanged existing domain tolerance. This pipeline builds different F32 block
interiors from the saved native board pair, so its time is independent evidence.
No compatible saved complete heavy F64 result supports a speedup claim.
The subsequent current-source single-pass review measures 197.840 s with an
explicit four-thread shared board/block budget; see
[the review](F32_REVIEW_TELEMETRY_2026-10-01.md) for the changed settings, CPU route
bottleneck, unsupported block coverage, independent validation and open F32 defects.

The OS monitor samples the Node process tree at approximately 1 Hz. Maximum
sampled simultaneous working set/private memory is 584,728,576 / 709,144,576
bytes; the compute child's OS peak working set is 517,828,608 bytes. These
measurements exclude the monitor and untracked driver/VRAM memory. Sampled
private bytes can underestimate a true private peak. Data and exact worker/source
settings are in v7/Telemetry-full-gpu.memory.json and the result manifest.

## Remaining ESPower route-quality review

The same isolated F64 bounded route instrument freezes 27 terminal obligations
on the saved F64 placement and compares v6 F32 raw poses. It reports penalty
6272.150125 -> 7075.092437, unresolved/budget-exhausted 3 -> 4, feasibilityOrder=1.
Critical D+ (R8.1 -> U12.A6) and D- (R7.1 -> U12.A7) change from found to
budget_exhausted; ordinary EN improves from budget_exhausted to found. A budget
exhaustion is not proof of NoPath, but these critical changes are an open quality
item, not hidden by improved overall HPWL elsewhere.

Board ranks on ESPower include two existing native hard counts. The count is
already the first integer ranking field, yet its additional 100-million offset
in the float score erases up to 16-unit soft improvements at this scale. Source
v8 removes that mathematically cancelling constant from both scorers, preserving
integer count priority, severity and its existing tie/penalty rules. Complete
ESPower quality is being rechecked after this numerical representation change.

## Scope correction after user review

The user rejected expanding the F32 migration into a new physical geometry
system. Removed `f32-interval.ts`, `physical-enclosure.ts`, physical endpoint
inflation at the native boundary, owner-relative rebasing, custom CPU/GPU body
materialization and the replacement intersection predicates. Existing geometry
algorithms now operate on F32. Keep coordinate localization before narrowing,
original locked output, canonical rounding/rotation, and conservative numerical
score bounds used by existing pruning. The latter do not inflate physical boxes.

The earlier full physical-error-chain requirement is withdrawn, not reported as
implemented. Existing frame restrictions and previously recorded ESPower route
quality limitations are unchanged, as requested. Historical runs above and the
197.840 s Telemetry review remain measurements of their recorded artifacts;
no new whole-board speedup is claimed for this cleanup.

The pre-cleanup source snapshot and binary Git diff are retained outside the
production tree in ignored `debugging/f32-cleanup-before-2026-10-01/`.

### Cleanup verification

- `npm run typecheck`: passed.
- Rust default-feature tests: 79 passed, 4 opt-in tests ignored.
- Rust `--no-default-features`: 74 passed, 2 opt-in tests ignored.
- `npm run native:build`: passed; one addon, 20,335,104 bytes
  (20.335 MB / 19.393 MiB). SHA-256: `3ed35779a2e597437a310eee823342fbde47feb6a4b7c31b0ff643e0cf35774f`.
- `pcb-f32-numerics.test.ts` + `pcb-fixed-placement.test.ts`: 10 passed on
  the final addon, including the real encoder at large offsets and locked output.
- Four selected real-GPU tests passed on the final addon: complete board CPU/GPU
  result, compound/layer/obstacle geometry, shared block-board-block runtime,
  and complete post-place scores/selection. All four require actual GPU execution;
  CPU fallback does not satisfy these tests.
- `pcb-pad-crossings.test.ts`, `pcb-block-quality.test.ts` and
  `pcb-board-alignment.test.ts`: 25 passed on the first cleanup release. The final
  release additionally retains overflow-safe integer angle subtraction when
  restoring block geometry; Rust and the above native/GPU tests ran afterward.
- `git diff --check`: passed. No new full Telemetry/CPU benchmark was run.

Logs are in `debugging/f32-cleanup-before-2026-10-01/`: `rust-tests.log`,
`rust-cpu-tests.log`, `build-final.log`, `native-numerics-tests.log`,
`gpu-tests.log`, and `ts-geometry-tests.log`. The second build was required by
that last integer-angle correction, not by repeated performance sampling.
