# P7-D1 current constraints — exact rank repair, full-control NO-GO

2026-10-31. Branch `feat/analyze`; starting HEAD `a483f1076ae657db86def032504097911719f91b`. No commit, push, subagent, reset, clean, or deletion of existing `debugging/` evidence. The parent-owned task file was not edited.

**Final status: NO-GO for the fixed RT0 recovery on the required slot basis.** The original DC singular refusal is explained by exact structural dependencies and its constraint basis is minimally repaired. The repaired DC matrix is full rank and the single permitted cold slot gets past `LOCAL_CONSERVATIVE_SINGULAR`, but the full raw residual audit then refuses with `LOCAL_CONSERVATIVE_RESIDUAL: nodal=1e0 cell=1e0`. Saved-basis replay without FEM proves that 70 of 73 current modes cannot simultaneously preserve all original nodal reactions, impose zero divergence in every cell, and insulate non-electrode DC boundaries. No loaded full/reduced state or section flux was produced. Continuous was not run.

This is not D1/P7/P8/P11 acceptance, current-conservation PASS, or a hybrid comparison. The old mixed P1 / constant-cell-current reference remains unaccepted. The independent magnetic patch1→2 failure `10.2046289% > 2%` remains unchanged and open.

## 1. Independent exact rank/dependency audit

Only saved `debugging/p7-closure/1793437157149686200/joint-fine.json` mesh, quotient-node, electrode, component, and current-basis data were used for diagnosis. No electric or magnetic solve was run for the audit.

The independent Python implementation reconstructs sorted geometric faces, one shared signed internal-face DOF, quotient-node reactions, cell divergences, electrode faces, connected components, and the old row order. Multiplying every row of `C` by three gives integer entries: nodal coefficients are signs and divergence coefficients are three times signs. This row scaling preserves rank and dependencies. Rank is independently computed modulo `1,000,000,007` and `1,000,000,009`; every reported null vector and canonical dependency is then checked by exact integer multiplication with residual infinity norm zero.

### Slot matrix evidence

| Matrix | Faces / free faces | Rows | Rank at both primes | Deficiency |
| --- | ---: | ---: | ---: | ---: |
| DC raw | 484 / 288 | 268 | 189 | 79 |
| DC old: one last cell row removed per component | 484 / 288 | 265 | 189 | 76 |
| DC repaired: proved star rows omitted and compatible component gauges | 484 / 288 | 189 | 189 | 0 |
| AC raw | 484 / 484 | 268 | 265 | 3 |
| AC old gauged basis, unchanged | 484 / 484 | 265 | 265 | 0 |

The slot has 82 potential DOFs, 186 cells, three connected components, six electrode quotient DOFs, and 76 non-electrode quotient DOFs. The raw DC deficiency is exactly `76 + 3`; the old component-only gauging therefore leaves all 76 additional dependencies. The raw and gauged modular nullspaces have 79 and 76 vectors respectively, all with exact integer residual zero.

### Canonical dependencies

For a non-electrode quotient node `v`, let `r_v` be its original nodal reaction constraint and `d_T` the divergence constraint of cell `T`. With zero flux on every non-electrode DC boundary face,

`3 r_v - sum_{T containing v} d_T = 0`.

To verify the identity, faces opposite `v` give the nodal contribution; internal faces containing `v` occur with opposite signs in the two cell stars and cancel; non-electrode boundary faces containing `v` are fixed to zero. The audit constructs this exact vector independently for each of the 76 non-electrode nodes. Every vector has exact residual zero.

For each connected component `K`, the original all-node/all-cell relation is

`3 sum_{v in K} r_v - sum_{T in K} d_T = 0`.

After eliminating the non-electrode star relations, it becomes

`3 sum_{electrode v in K} r_v + sum_{T in K} (k_T - 1) d_T = 0`,

where `k_T` is the number of distinct non-electrode potential DOFs among the four vertices of `T`. Thus removing an arbitrary last cell is not generally valid after the star elimination: its coefficient must be nonzero. The actual component evidence is:

| Component | Cells / nodes | Old gauge | Old reduced weight | Repaired gauge | Repaired reduced weight |
| --- | ---: | ---: | ---: | ---: | ---: |
| 0 | 30 / 18 | 29 | 2 | 29 | 2 |
| 1 | 30 / 18 | 59 | 3 | 59 | 3 |
| 2 | 126 / 46 | 185 | 0 | 184 | 1 |

Merely omitting the 76 star rows while retaining old gauges still gives rank 188 with one dependency. Selecting the last nonzero-weight cell per component gives rank 189 with no deficiency. This is a mathematical basis selection rule, not row dropping until Cholesky passes.

Every omitted non-electrode row follows from retained cell-divergence rows and insulation. Every removed DC gauge row follows from the corresponding reduced component relation. The production recovery retains its independent full raw nodal/cell/electrode/internal/boundary residual audit, so omitted rows are checked rather than assumed numerically. AC has no DC insulation star identity; its existing one-cell-per-component basis is already full rank and is unchanged.

### Manufactured-test coverage

The former six-tetrahedron fixture has no non-electrode potential DOF: every mesh vertex belongs to one of its two electrode quotients. Consequently the old manufactured tests cannot excite the slot's star dependency. The new center-fan 12-tetrahedron fixture has two non-electrode DOFs. Before the implementation repair it fails in `ConservativeSpace::new` with the same `LOCAL_CONSERVATIVE_SINGULAR`; after the repair it passes and explicitly checks the omitted nodal residuals and every cell divergence. Existing constant/affine/shared-face/complex/determinism/nonfinite tests remain unchanged and pass.

Artifacts: `rank_audit.py`, `slot-rank-audit.json`, `regression-before.log`, and `regression-after.log` under `debugging/p7-d1-current-constraints/`.

## 2. Minimal implementation diff

Only two pre-existing source files changed:

1. `native/analyze/src/extraction/local/conservative_current.rs`
   - DC retains electrode reaction rows and cell rows, except the proved component gauge.
   - DC non-electrode reaction rows are omitted only by the exact star identity above.
   - DC gauges use the last cell with `k_T-1 != 0`; if the reduced component relation vanishes, no cell is removed.
   - AC retains the previous nodes and one-last-cell-per-component gauge.
   - The full original raw residual audit remains intact.
2. `native/analyze/tests/conservative_current.rs`
   - One regression with two non-electrode quotient DOFs and full omitted-row/cell checks.

Source diff stat before this report/changelog: 41 insertions and 6 deletions across those two files. No mixed PDE, magnetic, energy/loss, RLC, reduction, geometry, electrode totals, boundary conditions, allocation guard, solver, tolerance, hybrid/interface, P6, DSL, or public selector changed. No regularization, pseudoinverse, empirical scaling, arbitrary row dropping, F32/GPU, external solver, or out-of-core path was added.

## 3. Exact remaining compatibility blocker

The cold run reaches conservative basis projection and refuses the full raw residual. Independent saved-packet replay explains why the required constraints are inconsistent for the existing reduced basis.

Zero divergence in all cells plus insulated non-electrode boundaries imply, through the star identity,

`r_v = 0` at every non-electrode quotient node.

The saved slot `currentBasis` has 73 modes. Modes 0–2 satisfy the necessary component/non-electrode conditions at the `1e-8` normalized threshold. Every mode 3–72 violates it: normalized non-electrode reactions range from `0.16264689575375366` to `1.0`. The first incompatible mode is mode 3:

| Quantity | Mode 3 value |
| --- | ---: |
| Maximum absolute nodal reaction | `20.430809901176136 A` |
| Maximum absolute non-electrode reaction | `4.4560390742099445 A` |
| Normalized non-electrode reaction | `0.21810388798896438` |
| Component-total magnitudes | `4.996e-16`, `4.857e-16`, `1.332e-15 A` |

The harmonic modes 0–2 have normalized non-electrode residuals `1.042623832e-12`, `6.401884081e-13`, and `1.772934213e-12`. The maximum absolute component total over all 73 modes is `3.623767952e-13 A`; the blocker is specifically the non-electrode reactions, not component current totals.

`board_prepare.rs` calls `project_basis(..., basis.current, true)` in mode order. The cold residual is therefore consistent with reaching mode 3 after the three compatible harmonic modes, although the runner does not serialize the failing mode index. No repaired constraint factor can make an incompatible target satisfy the omitted raw rows. Repairing this would require changing the current basis/formulation or abandoning preservation of all original nodal reactions, both outside this task. No alternative was attempted.

Artifacts: `saved_basis_compatibility.py`, `saved-basis-compatibility.json`, and the opened `saved-basis-compatibility.png`. The plot shows modes 0–2 near roundoff and modes 3–72 above the compatibility threshold by many orders of magnitude.

## 4. Build and focused verification

Commands from repository root:

```sh
PATH="/c/Users/kiril/.cargo/bin:$PATH" cargo test --manifest-path native/analyze/Cargo.toml --test conservative_current --test board_response --test board_prepare
PATH="/c/Users/kiril/.cargo/bin:$PATH" npm run native:build
PATH="/c/Users/kiril/.cargo/bin:$PATH" cargo build --release --manifest-path native/analyze/Cargo.toml --example conservative_current
npm run typecheck
git diff --check
```

| Check | Result | Wall time |
| --- | --- | ---: |
| New regression before repair | Expected FAIL: `LOCAL_CONSERVATIVE_SINGULAR` | 2.795 s command |
| New regression after repair | PASS | 20.185 s command |
| Final focused Rust tests | PASS: 18/18 (`5+8+5`) | 3.300 s |
| Native build without Cargo in PATH | Environment FAIL: `spawnSync cargo ENOENT` | 0.912 s |
| First native build with PATH | Tool timeout during final compilation | 300.568 s |
| Incremental native build | PASS | 14.953 s |
| Release cold-runner build | PASS | 79.683 s |
| TypeScript typecheck | PASS | 7.623 s |
| `git diff --check` | PASS; LF→CRLF warnings only | 0.320 s including hashes |
| Python numerical audits/plot | PASS; plot opened and inspected | 1.635 / 0.902 / 4.246 s |

The old compiler warnings remain; no unrelated warning cleanup was made.

## 5. Single cold slot and resources

The one new cold attempt used the unchanged slot board/request, no checkpoint, no prepared/field cache, one native thread, `300 s` deadline, `2 GiB` private cap, and `1 GiB` host reserve. The supervisor confirms no child processes and samples RSS/private commit every 20 ms.

| Quantity | Result |
| --- | --- |
| Wall | `12.953 s` |
| Exit | 1, `LOCAL_CONSERVATIVE_RESIDUAL: nodal=1e0 cell=1e0` |
| Peak sampled RSS | `414396416 B` (`395.20 MiB`) |
| Peak private commit | `411074560 B` (`392.01 MiB`) |
| Initial commit headroom / available physical | `22261276672` / `1847619584 B` |
| Resource/time bounds | PASS |
| DC singular barrier | Passed; no `LOCAL_CONSERVATIVE_SINGULAR` |
| Full raw current residual | **NO-GO** |
| Output `slot-full.json` | Not created; SHA256 null |

Continuous was **not run**, as required after slot failure. No magnetic/domain refinement, hybrid, STM32, direct DSL, profile3, or additional cold/retry run was started.

## 6. Loaded states and complex section audit

The required sections remain fixed at `x=0.500123/1.000123/1.500123 mm`, normal `+x`, with `d=[1,0,-1,0] A` RMS and four `50 ohm` loads. Because the cold run failed before basis projection completed, no exact loaded full/reduced potential/current/face state exists.

| Variant | State | Sections | BOTTOM / TOP / all-conductor flux | Algebraic error | Full/reduced error |
| --- | --- | --- | --- | --- | --- |
| Slot | loaded DC / 1 kHz | all three | N/A — no loaded state | N/A | N/A |
| Continuous | loaded DC / 1 kHz | all three | NOT RUN | NOT RUN | NOT RUN |

The independent complex BOTTOM/TOP/all-conductor section audit was not executed because its required full states do not exist. Old reduced packets were not substituted for new results. No current-conservation or observable gate is claimed.

## 7. Gate ledger

| Gate | Result |
| --- | --- |
| Exact slot rank/dependency cause | PASS: 79 raw dependencies = 76 stars + 3 component relations |
| Repaired DC structural rank | PASS: 189 rows, rank 189 at two primes |
| AC structural rank | PASS: 265 rows, rank 265; implementation unchanged |
| Manufactured regression reproducing old dependency | PASS: fails before, passes after |
| Existing + new focused tests | PASS: 18/18 |
| Native build / release runner / typecheck / diff check | PASS after recorded environment/timeout handling |
| Single cold slot time/resources | PASS |
| Full raw current constraints `1e-8` | **NO-GO: saved modes 3–72 incompatible; cold residual refusal** |
| Loaded full/reduced DC / 1 kHz states | **NO-GO: not produced** |
| Complex section continuity/electrode checks | **NO-GO: unverified, no states** |
| Full-versus-reduced observable `2%`, floor `1e-6 A` | **NO-GO: unverified** |
| Continuous | NOT RUN |
| Magnetic patch1→2 `2%` | **NO-GO remains: `10.2046289%`** |
| P7/P8/P11, hybrid/interface | UNCHANGED / NOT CLAIMED |

## 8. Identity and hashes

| Item | SHA256 |
| --- | --- |
| Saved slot packet | `d878292e731ba48e6d319c70a500b1bc8c54c280b2f719341dde72f803224033` |
| Slot board | `f6d066e474f70e9f68388abe08c2f54606148b6792377c7748630869664613f5` |
| Continuous board, not run | `3cb149436375a86340d9ac4466e509d9f9853d46793f2b859c9cc741b554ebf0` |
| Shared request | `fdbc0bcc1152ed859025ef4044ea6d14c30dc7cb9c68105751122a7632885840` |
| New addon | `c3dfcba45fd1008d9317d632cd4c18ebe076b0cacaa5ce596dc10564deeb8014` |
| New cold runner | `70efdd4cbf5b64fd92f6307dac666207d1b46e6f9bb0920839d6a5582d6935ae` |
| Repaired implementation | `6aedd9c909e3ffa3e85529109a7007be1ca529615a14f72489ab804fb7f79c8e` |
| Focused conservative test | `9a5c46cbc3fe87aee0602683564356b595197aac7badced9bbb8287e29a9657f` |
| Rank audit script / JSON | `2d63997f025a6354375f1dbdb127981e3ea3c6f644ab45ebe14aa52820de7b75` / `c5f471509f788ae7e1c5f180877aedea45fa040111911e9fcbbf133e697531e8` |
| Compatibility script / JSON | `c952dd3630773f14f6a39a17fabe062b111c6489dfebe4e1faad3d53dff4efc0` / `3a35c1b091a721ceae3504587122a914db4e1cbdc4894bd7e6989b49ffe519f5` |
| Compatibility plot | `5eeec2ce10f23d9d5054052bf669112ff7fe7c8cd55109d806eed7d72aaf370e` |
| Cold supervisor / log | `2d9085ee9f395c1482541c7902a564a1eee2125863dacb4dcfd0dfabe71cce83` / `81204ff0e5ba659c346adab6a8f96df381417fcf98ae5ad01c5d6acdeb49b424` |
| Final focused log | `6d4032a3fbd734fc442de48e803b300475c11a22ef000982a247d579e2c04c66` |
| Successful native build log | `f30f3b0d39c17cfb35003664f493b67b0b7c105876b856a5faf9293048d3027c` |
| Release runner build log | `08258b4c6d8267ec2256db944abf4baf0695f6b4563a02cb64f54758db3a5ed3` |
| Typecheck log | `521d92075803206be761413d90345a11e020ea8196517a8038ad3f631c05dec9` |

All new local artifacts are confined to `debugging/p7-d1-current-constraints/`. The cold attempt is new and cold; the rank and compatibility evidence explicitly reuses the old saved packet and is labeled as such.

## 9. Stop for parent review

The rank-selection defect is proved and minimally corrected, but the selected RT0 recovery remains incompatible with the existing full reduced current basis under the mandatory reaction/divergence/insulation contract. Stop here for parent review. No further formulation, solver campaign, continuous run, or next stage is authorized by this result.

**Parent review, 2026-10-31:** the two-prime rank audit was independently replayed and the 18 focused Rust tests and TypeScript typecheck passed. The remaining result is specifically a rejection of **per-mode DC projection of every auxiliary magnetic/training basis column**. It does not prove that a compatible combined physical loaded state cannot be recovered. Auxiliary modes need not be standalone insulated DC fields. Any continuation must recover and audit the actual combined full/reduced loaded states rather than retain the unnecessary all-mode DC projection requirement.

CURRENT_CONSTRAINTS_NO_GO
