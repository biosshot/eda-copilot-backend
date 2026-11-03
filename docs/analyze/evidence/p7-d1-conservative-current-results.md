# P7-D1C conservative normal-current recovery — full-control NO-GO

2026-10-31. Branch `feat/analyze`; starting HEAD `c12701f2923f11a232de3ce907061b32ab7f5071` (`chore(analyze): checkpoint D1 reference current audit`). No commit or push was made.

**Final physical status: NO-GO for the selected conservative-current implementation.** The focused manufactured tests and builds pass, but the first cold full-space slot control refuses the equilibration factor with `LOCAL_CONSERVATIVE_SINGULAR: independent equilibration constraints are not positive definite; no regularization`. No loaded full-space state, section flux, or full-versus-reduced observable was produced. The continuous control was not run after this blocker. This is not a claim that every possible conservative formulation is impossible; it is a reproducible failure of the single selected bounded RT0 minimum-correction implementation on the required mesh.

The old mixed P1 / constant-cell-current result remains unaccepted. The independent magnetic patch1→2 failure `10.2046289% > 2%` remains unchanged and independently open. No hybrid/interface comparison, STM32, direct DSL, refinement campaign, P7/P8/P11 closure, or new physical-accuracy claim is made.

## 1. Fixed formulation and mathematical scope

The original PDE, mesh, electrode quotients, mixed solve, reduction, RLC matrices, magnetic operator, Joule losses and energy calculations are unchanged. The addition is an observable-only lowest-order Raviart–Thomas (RT0) recovery on the existing conforming copper mesh, enabled only in diagnostic `mode="full"`.

For a geometric triangular face, sorted mesh vertex IDs define one global normal `n_g`. For incident tetrahedron `T`, local opposite vertex `k`, volume `V_T > 0`, and orientation `s_T = sign(n_g · n_T,out)`, the recovered density is

`J_RT(x) = sum_F s_T f_F (x - p_k)/(3 V_T)`.

Here `f_F` is the signed physical face flux in A, `q_T,F = s_T f_F` is the outward tetrahedral flux, and `J_RT` is A/m². One internal face has one global DOF shared by both cells, hence `q_A + q_B = 0` identically. Real and imaginary RMS phasor parts use the same real linear recovery operator.

The existing cell current coordinate `i_T` gives a weak nodal reaction

`r_T,v = i_T · incidence_T,v`.

For the constant cell vector this reaction is **one third** of the physical flux through the face opposite `v`, not the complete face flux. Therefore the initial globally oriented face value is

`f0_F = average_T (3 s_T r_T,v)`,

and a recovered outward face flux contributes `q_T,F/3` to the opposite nodal reaction. Recovery imposes zero physical cell divergence,

`sum_F q_T,F = 0`,

while preserving every original nodal reaction. At DC all non-electrode copper boundary fluxes are fixed to zero and electrode quotient totals are preserved. At AC charged boundary faces remain unknowns: their flux represents the already existing charge/displacement reaction. No charge, volume source, ground, regularization, or empirical scale is added.

The deterministic selected solution is

`min sum_free (f_F - f0_F)^2`,

subject to those constraints. With constraint matrix `C`,

`f = f0 + C^T (C C^T)^+ (b - C f0)`.

The implementation removes exactly one known dependent cell row per connected component, forms bounded sparse `C C^T`, and performs two residual corrections with the same factor. It then independently checks all nodal reactions, all cell divergences (including removed rows), electrode totals, internal ownership, finite/range conditions, and DC insulation with normalized tolerance `1e-8`. No pseudoinverse regularization or alternative method was introduced after the real-mesh singular refusal.

For a mesh-aligned planar section, linearity inside each tetrahedron permits exact integration by triangulating the cell-plane polygon and evaluating `J_RT` at each triangle centroid:

`Phi_S = sum_triangles A_triangle J_RT(centroid_triangle) · n_S`.

At DC, zero cell divergence, internal equal/opposite flux and insulated boundaries imply the section/electrode balance. At AC, section-minus-electrode conduction is reported as the charge/displacement contribution; section flux is not silently required to equal electrode current.

## 2. DOF ownership, orientation and preservation boundary

- `Face { nodes, cells, electrode }` owns one sorted geometric triangle and its one or two incident cells.
- `FaceCell { cell, opposite, sign }` owns the local opposite vertex and global-to-outward orientation.
- The mesh and cell ordering are not mutated. A focused test compares every original node coordinate bit and cell node tuple before/after recovery.
- Original KCL/KVL/power admission remains in `volume_solve::Solver`; no solver guard or tolerance was edited.
- Original cell-current loss, reduced RLC, magnetic action and energy remain based on the old current coordinates. RT0 flux is never substituted into those quantities.
- `board_response` reconstructs `conservativeFaceRealImaginaryA` only when the new optional `conservativeCurrent` packet exists. Old packets take the original path unchanged.
- The recovery certifies only algebraic properties of the new observable. It does not certify continuum accuracy, old/new energy equivalence, magnetic convergence, or hybrid/interface agreement.

## 3. Implementation actually present

1. `native/analyze/src/extraction/local/conservative_current.rs`: RT0 topology, separate DC/AC bounded projectors, recovery, basis projection and cell-divergence audit.
2. `native/analyze/src/extraction/local/board_prepare.rs`: full-only topology and DC/AC projected bases; exact AC potential (`shape + correction + component constants`), full/reduced currents and face arrays; four unit-port drives for four ports, while retaining the historical two-port drive order; original harmonic electrode recovery basis for loaded DC.
3. `native/analyze/src/extraction/local/board_response.rs`: optional bounded DC/AC face-state replay with shape/nonfinite/output-budget validation.
4. `native/analyze/examples/conservative_current.rs`: cold four-port, single-frequency `1 kHz` full capture with no checkpoint or field/prepared cache.
5. Focused tests in `native/analyze/tests/conservative_current.rs` and `board_response.rs`.
6. Diagnostic scripts and fixed inputs under `debugging/p7-d1-conservative-current/`; no fixture or old evidence was modified.

The loaded-state audit implements an independent six-electrode full-space DC conductance solve, exact AC superposition through `(I + U/R)a = d`, independent dense reduced replay, RT0 polygon integration, internal/nodal/cell/electrode checks and full/reduced errors. It is syntactically validated but could not be executed against real full states because the slot capture produced no output packet.

## 4. Focused evidence before the full attempt

Command:

```sh
PATH="/c/Users/kiril/.cargo/bin:$PATH" cargo test --manifest-path native/analyze/Cargo.toml --test conservative_current --test board_response --test board_prepare
```

Final result: **17 passed, 0 failed** (`conservative_current`: 4; `board_response`: 8; `board_prepare`: 5).

| Focused gate | Result | Evidence |
| --- | --- | --- |
| Constant insulated DC current, known electrode/section flux | PASS | Exact constant mode recovered on six tetrahedra; non-electrode boundary faces exactly zero |
| Affine RT0 basis and oriented face/section integrals | PASS | Affine `J=a+c*r` basis/quadrature test; this test field has nonzero divergence and is not mislabeled as a divergence-free recovered constant-cell state |
| Shared internal face, equal/opposite normal flux | PASS | One signed global DOF; independent reaction accumulation uses `q/3` |
| Complex real/imaginary states and determinism | PASS | Bit-identical repeated recovery; original nodal reactions preserved |
| Nonfinite/range/wrong-length refusal | PASS | Explicit errors; no regularized fallback |
| Unchanged physical geometry identity | PASS | Node bits and cell connectivity unchanged |
| Old saved-packet response compatibility | PASS | Existing response tests pass without `conservativeCurrent` |
| New optional DC/AC replay, malformed basis refusal | PASS | Focused response regression passes |
| Audit/runner/plot Python syntax | PASS | `python -m py_compile ...` completed; numerical audit/plot not run because no full output exists |

## 5. Cold full-space control and exact blocker

The fixed board/request hashes matched the previous physical identities. The runner used `entity:6:8:16:3`, air degree 3, one native thread, no checkpoint, no prepared cache and no field cache. Existing-attempt refusal prevents an unrequested rerun.

Command:

```sh
python debugging/p7-d1-conservative-current/run_cold.py slot
```

Result: **NO-GO**, exit code 1 after `11.625 s`. The log reaches geometry, unchanged current assembly, electric and magnetic factors, initial magnetic basis, full solve preparation and reduction (`6+9` modes), then fails while constructing the conservative projector:

```text
Error: "LOCAL_CONSERVATIVE_SINGULAR: independent equilibration constraints are not positive definite; no regularization"
```

Thus the failure occurs before DC/AC basis projection and before any unit-port loaded-state capture. `slot-full.json` does not exist and its hash is null. The error does not distinguish DC from AC projector construction internally; no second run was used to refine that diagnostic. The precise architectural blocker is that the selected independent `C C^T` factor is not positive definite on the actual slot topology after the permitted one-cell-row-per-component gauging. The task forbids silently regularizing or switching methods, so execution stopped.

The continuous cold control was **not run**. No hybrid/STM32/direct-DSL or refinement work was started.

## 6. Required loaded DC / 1 kHz flux tables

No numerical flux or area was obtained from the new recovery. The following entries are deliberately not filled from the old reduced packets.

| Variant | State | Section `x` (mm), normal `+x` | Area (m²) | TOP (A) | BOTTOM (A) | all-conductor (A) | algebraic error | full/reduced error |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| Slot | loaded DC | 0.500123 | N/A | N/A | N/A | N/A | N/A | N/A |
| Slot | loaded DC | 1.000123 | N/A | N/A | N/A | N/A | N/A | N/A |
| Slot | loaded DC | 1.500123 | N/A | N/A | N/A | N/A | N/A | N/A |
| Slot | loaded 1 kHz | 0.500123 | N/A | N/A | N/A | N/A | N/A | N/A |
| Slot | loaded 1 kHz | 1.000123 | N/A | N/A | N/A | N/A | N/A | N/A |
| Slot | loaded 1 kHz | 1.500123 | N/A | N/A | N/A | N/A | N/A | N/A |
| Continuous | loaded DC / 1 kHz | all three fixed sections | N/A | N/A | N/A | N/A | N/A | not run |

The requested drive remains `d=[1,0,-1,0] A` RMS with four `50 ohm` loads. No exact loaded potential/current/face state or replay result is claimed. AC charge/displacement tables likewise remain uncomputed. No new plot was generated or opened, because there is no new numerical audit to plot; the old `reference-current-audit.png` was not reused as new evidence.

## 7. Gate ledger

| Gate | Result |
| --- | --- |
| Small manufactured/geometry/determinism/nonfinite tests | PASS |
| Native addon and release runner build | PASS |
| TypeScript typecheck | PASS |
| `git diff --check` | PASS |
| Cold slot resource/time bounds | PASS |
| Slot conservative `C C^T` factor | **NO-GO: singular refusal** |
| Exact loaded full-space DC / 1 kHz states | **NO-GO: not produced** |
| Full algebraic continuity/electrode constraints `1e-8` | **NO-GO: unverified on full fixture** |
| DC section/electrode balance `1e-8` | **NO-GO: unverified on full fixture** |
| Full-versus-reduced cells/faces/sections `2%`, floor `1e-6 A` | **NO-GO: unverified** |
| Continuous control | NOT RUN after slot blocker |
| Magnetic patch1→2 `2%` | **NO-GO remains: `10.2046289%`** |
| P7/P8/P11 or hybrid/interface status | UNCHANGED / NOT CLAIMED |

## 8. Resources, commands and hashes

### Cold slot resources

- Wall: `11.625 s`; internal geometry `0.011697 s`, electric assembly/factor `0.011900 s`, magnetic assembly/factor `8.015291 s`, initial magnetic basis `2.374078 s`, reduction `1.086683 s`.
- Peak sampled RSS: `414859264` bytes (`395.64 MiB`); peak private commit: `411144192` bytes (`392.08 MiB`).
- Initial available commit: `19390427136` bytes; initial available physical RAM recorded by the supervisor: `3966451712` bytes.
- Bounds: `300 s`, `2147483648` bytes private, `1073741824` bytes host reserve; one thread; 20 ms sampled lower bound; no child processes.
- Cold status: no cache/checkpoint reuse; existing attempts are refused.

### Build and static checks

- `npm run native:build`: two 300 s tool timeouts during final thin-LTO compilation; the third unchanged incremental command passed in `168.910 s`. Build profile and resource policy were not changed.
- `cargo build --release --manifest-path native/analyze/Cargo.toml --example conservative_current`: PASS, `70.047 s`.
- `npm run typecheck`: PASS, `6.542 s`.
- Final focused tests: PASS, `23.267 s` command wall time.
- `python -m py_compile debugging/p7-d1-conservative-current/{audit.py,plot.py,run_cold.py}`: PASS.

### Identity and binary hashes

| Item | SHA256 |
| --- | --- |
| Initial addon | `f15e8aee3c87da0847902c571199a9bc2eaa5f116bf3bb957f686fb9d727ee73` |
| New addon `eda-copilot-node.win32-x64-msvc.node` | `63c743c85d3f5c7d56d05e4eb101f08806634bb1164a6a7ad56d2b9373a5c4ea` |
| Cold runner `conservative_current.exe` | `ce5e73c41b05eaeb83fd7dce561ee8da05681d9895578ccef922a1877484162e` |
| Slot board | `f6d066e474f70e9f68388abe08c2f54606148b6792377c7748630869664613f5` |
| Continuous board | `3cb149436375a86340d9ac4466e509d9f9853d46793f2b859c9cc741b554ebf0` |
| Shared slot/continuous request | `fdbc0bcc1152ed859025ef4044ea6d14c30dc7cb9c68105751122a7632885840` |
| `conservative_current.rs` implementation | `34e027acb2ea3c0870643c19d138eda0bc601e022b3304c87d03b3fd28bdcf00` |
| `board_prepare.rs` | `8c77b982fb7fe8cda10c70bbaa1b5f6e0226d8b208eda5a2d10181d824dd2c16` |
| `board_response.rs` | `18448cc66511a6e9c2913c1a381f319b0a42713c01695e8e090d9e044e711530` |
| Conservative focused test | `7691dc884068deb2cd8484c798372f7b14f087596084db17479bb34b0b036a84` |
| Response focused test | `aacd3f16180044a9943b56e165d3e1a6550b0a35038e8836bc8ac6a462b9a646` |
| Cold example | `bfd0a46fb8d8adb1ca986c2ec9fcd814b40e79357724fd0f0c796ee29d1f3a1b` |
| Independent audit script | `5bfe8355103d7a246fae8107b5ecedf32538ab36538737c46f89d65ddee4c81d` |
| Cold supervisor | `75ae6c77a82a84f804e9049ff6b73b9c532c7061bd251e0d9071ca414083cf1c` |

The old identity core strings (`d63946281d8dfb61c14d71bbbe0f3dc565d52c1b4eb2944fede9ccf1cb08d0a0` slot and `c6a39ed731e92cbf68449cb5fb6683f9127125f2b1d3619cae3e0a94bef90048` continuous) are historical provenance only. The new source state is HEAD `c12701f...` plus the uncommitted diff listed above; no commit was created.

## 9. Stop condition and handoff

The small tests establish that the implemented RT0 basis and reaction normalization are mathematically consistent on the manufactured mesh. They do not override the actual full-fixture singular factor. The required next decision is parent review of this selected method's rank/deficiency blocker; this task does not authorize regularization, another recovery family, PDE/mesh replacement, or a refinement campaign. All full numerical acceptance items remain open, and the previous magnetic blocker remains independent.

CONSERVATIVE_CURRENT_NO_GO
