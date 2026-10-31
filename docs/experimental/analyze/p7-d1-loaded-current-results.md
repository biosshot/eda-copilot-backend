# P7-D1: loaded full/reduced conservative RT0 current

2026-10-31. Checkpoint `8ecb3d8ebabb0b30b9feb4809bc010d740e69a13`; executor stopped after the sole permitted cold slot for parent review. No commit, push, reset, clean, subagent, hybrid, magnetic/domain refinement, STM32, direct DSL, profile3, or repeated numerical attempt was started.

## 1. Parent correction and outcome

The former `project_basis(..., basis.current, true)` refusal established only that an auxiliary magnetic/training current column need not be an independent insulated DC physical field. It did not establish that the physical loaded linear combination is incompatible with RT0 recovery.

This task removes that all-mode requirement and implements the same state-based RT0 recovery for actual full/reduced loaded DC and 1 kHz states. A manufactured regression proves that two individually incompatible auxiliary DC modes can have a compatible loaded combination with complete raw-row, cell-divergence, shared-face, insulation, electrode, and determinism checks.

The implementation and 20 focused tests pass. The sole cold slot completes all four exact full/reduced 1 kHz unit-port states, but the subsequent loaded DC six-electrode solve is rejected by the existing bounded complex LU guard:

```text
NETWORK_SINGULAR: nonfinite/rank deficient or ill-conditioned system; condition=111045687460217.7
```

The measured condition estimate is `1.110456874602177e14`, above the unchanged `1e12` guard. No loaded-state record, `slot-full.json`, independent numerical audit, or section-flux result was produced. **The loaded-current result is NO-GO at the loaded DC electrode solve. Continuous was not run.** This is neither a repetition of the old standalone auxiliary-mode refusal nor evidence that loaded RT0 recovery itself failed: loaded RT0 recovery was not reached.

The separate magnetic patch error `10.2046289% > 2%` remains unchanged. No conditional joint reference candidate, D1/P7/P8/P11 acceptance, or hybrid authorization is claimed.

## 2. Loaded-state equations and conventions

All phasors and drives are RMS. The fixed internal four-port Norton drive is `d=[1,0,-1,0] A`, with `R_p=50 ohm` on every port. This is the internal four-port arrangement, not the public from/to two-port contract.

For six harmonic electrode states `h_e`, define the full electrode conductance and port incidence as

```text
G[i,e] = reaction_at_electrode_i(sigma*E(h_e))       [A/V]
B[a,p] = +1 for the positive terminal, -1 for return.
```

The loaded full DC electrode voltages satisfy

```text
(G + B*diag(1/R_p)*B^T) * V = B*d.
```

The full state is `v=sum_e V_e*h_e` and `I=sigma*E(v)`. Reduced DC uses the existing coupled model, the same drive, and the same four port admittances. Reduced full-space potential/current are reconstructed from `voltageBasis` and `currentBasis` using the resulting reduced coordinates.

For AC, `U[q,p]` is full port voltage `q` in the exact unit-drive state `p`. The source coefficients satisfy

```text
(I + diag(1/R_p)*U) * a = d.
```

The same complex coefficients `a` combine full and reduced unit-port states. AC `reducedState` has separate real and imaginary blocks, not one complex vector of length `2n`. If `x_p=[x_re_p,x_im_p]`, the loaded blocks are

```text
x_re = sum_p (Re(a_p)*x_re_p - Im(a_p)*x_im_p)
x_im = sum_p (Re(a_p)*x_im_p + Im(a_p)*x_re_p).
```

Reduced potential/current are reconstructed from these final blocks. RT0 recovery is applied only after loaded solve/superposition: insulated recovery for final DC real vectors and charged-surface recovery for final AC real and imaginary vectors. There is no all-mode DC projection and no per-mode conservative AC basis in the v2 diagnostic packet.

The new `rt0-loaded-state-v2` packet stores face topology and sign metadata, drive/load/coefficient provenance, exact full/reduced raw potential/current arrays, final recovered face arrays, port voltages, and reduced states. `board_response` retains legacy v1 basis replay for old packets. A v2 packet is diagnostic evidence for its recorded state and is not reinterpreted as an arbitrary-drive basis; existing current, loss, port, RLC, and magnetic response paths remain unchanged.

## 3. Implementation diff

- `native/analyze/src/extraction/local/board_prepare.rs`: removes `project_basis` and `dcBasis`/`acBasis`/`dcElectrodeBasis`; adds loaded DC/AC state combination and final-state RT0 recovery; serializes v2 records. The optional diagnostic is restricted to the existing four-port full capture containing 1 kHz.
- `native/analyze/src/extraction/local/board_response.rs`: preserves legacy v1 replay, validates v2 DC/1 kHz records, and omits unaccepted arbitrary-drive conservative face output for v2 without changing old observables.
- `native/analyze/src/electrical/frequency/mod.rs`: makes the existing bounded complex LU helper crate-visible; no numerical algorithm or guard changes.
- `native/analyze/tests/conservative_current.rs`: adds the incompatible-auxiliary/compatible-loaded manufactured regression, including two omitted non-electrode rows and electrode totals.
- `native/analyze/tests/board_response.rs`: adds v2 compatibility/refusal coverage and retains the legacy v1 test.
- `debugging/p7-d1-loaded-current/`: contains only the new requests, bounded supervisor, state-based independent audit implementation, plot implementation, and the sole cold-slot evidence. Old `debugging/` directories were not modified or deleted.

The independent audit was adapted to recompute loaded DC/AC raw states and reduced real/imaginary block replay, then check the saved final RT0 arrays against complete nodal/cell/electrode/shared-face constraints. It was syntax-checked but could not perform a numerical audit because the native runner produced no output JSON. Consequently no numerical plot was generated or opened; no empty or fabricated flux table is reported.

## 4. Tests, builds, and static checks

Commands from repository root:

```sh
PATH="/c/Users/kiril/.cargo/bin:$PATH" cargo test --manifest-path native/analyze/Cargo.toml --test conservative_current --test board_response --test board_prepare
PATH="/c/Users/kiril/.cargo/bin:$PATH" npm run native:build
PATH="/c/Users/kiril/.cargo/bin:$PATH" cargo build --release --manifest-path native/analyze/Cargo.toml --example conservative_current
npm run typecheck
python -m py_compile debugging/p7-d1-loaded-current/audit.py debugging/p7-d1-loaded-current/run_cold.py debugging/p7-d1-loaded-current/plot.py
git diff --check
```

| Check | Result | Command wall time |
| --- | --- | ---: |
| Final focused Rust tests | PASS: 20/20 = 6 conservative + 9 response + 5 prepare | 30.508 s |
| First native build | Tool timeout during final addon compilation; no code error | 300.439 s |
| Incremental native build | PASS | 231.716 s |
| Release cold-runner build | PASS | 85.890 s |
| TypeScript typecheck | PASS | 7.252 s |
| Python syntax + `git diff --check` | PASS; LF-to-CRLF warning only | 0.423 s |

Existing compiler warnings remain; no unrelated warning cleanup was made.

| Binary/input | SHA256 |
| --- | --- |
| `native/node-addon/eda-copilot-node.win32-x64-msvc.node` | `5fa12c017c9b96d416b0f6bc74b26b248f0a177394745fef37f84203a9f69278` |
| `native/analyze/target/release/examples/conservative_current.exe` | `e7fa1fa87ac3d0c645f5707edd9d7a40c43772e907924e57c9643b393ee728dd` |
| Slot request | `fdbc0bcc1152ed859025ef4044ea6d14c30dc7cb9c68105751122a7632885840` |
| Continuous request (not run) | `fdbc0bcc1152ed859025ef4044ea6d14c30dc7cb9c68105751122a7632885840` |
| Slot board | `f6d066e474f70e9f68388abe08c2f54606148b6792377c7748630869664613f5` |
| Slot full output | Not created; SHA256 null |

## 5. Sole cold slot and resources

`python debugging/p7-d1-loaded-current/run_cold.py slot` started at `2026-10-31T17:24:39Z`. It used no cache/checkpoint, one thread, the release runner above, a 300 s supervisor deadline, a 2 GiB private cap, and a 1 GiB host commit reserve.

| Quantity | Observed |
| --- | ---: |
| Status / exit | `child-failure` / 1 |
| Wall time | `31.063 s` |
| Peak sampled RSS | `414404608 B` (`395.20 MiB`) |
| Peak private commit | `411058176 B` (`392.01 MiB`) |
| Initial commit headroom | `21532004352 B` |
| Initial available physical memory | `5972176896 B` |
| Resource/time bounds | PASS |
| Four full/reduced 1 kHz unit states | PASS; `developerVerification 19.156937 s` |
| Loaded DC six-electrode LU admission | **NO-GO: condition `1.110456874602177e14 > 1e12`** |
| Loaded DC/AC RT0 recovery | Not reached |
| Continuous | Not run, as required after slot failure |

The log's four full solves have normalized KCL no greater than `1.4786621023573797e-14`; their final normalized KVL values are `8.363318567943561e-10`, `5.024899898207124e-12`, `7.100623028900755e-10`, and `2.4051952954216522e-11`. These are unit-state solver diagnostics only, not loaded-current acceptance evidence.

Artifacts: `slot-cold.log`, `slot-supervision.json`, fixed request JSON, and the prepared audit/plot scripts under `debugging/p7-d1-loaded-current/`. There is no `slot-audit.json` or `conservative-current-audit.png`, because the required native loaded-state source does not exist.

## 6. Gate ledger

| Gate | Result |
| --- | --- |
| Parent correction: no standalone auxiliary DC requirement | PASS |
| State-based full/reduced loaded DC/AC implementation | PASS by code/focused tests |
| Auxiliary incompatibility with compatible loaded combination | PASS in manufactured regression |
| Shared-face, omitted-row, cell, electrode, insulation, determinism, nonfinite coverage | PASS in focused tests; previous AC complex coverage retained |
| Legacy v1 packet replay and unchanged old observables | PASS |
| v2 is not malformed arbitrary-drive basis output | PASS |
| Focused tests / native build / release runner / typecheck / diff | PASS |
| Sole cold slot time and memory bounds | PASS |
| Loaded DC electrode solve `1e-8` residual evidence | **NO-GO: rejected by unchanged condition guard before residual/state output** |
| Loaded full/reduced DC and 1 kHz algebraic continuity `1e-8` | NOT PRODUCED |
| Independent loaded-state replay `1e-8` | NOT RUN: no output JSON |
| DC section/electrode balance `1e-8` | NOT PRODUCED |
| Full/reduced current observable `2%`, floor `1e-6 A` | NOT PRODUCED |
| Complex BOTTOM/TOP/all-conductor section tables and areas | NOT PRODUCED |
| Continuous control | NOT RUN |
| Current-conservation PASS / conditional joint reference candidate | **NO-GO / not authorized** |
| Separate magnetic patch `10.2046289% > 2%` | Unchanged blocker |

Stop here for parent review. No automatic follow-up numerical attempt is permitted by this task.

**Parent review, 2026-10-31:** the loaded DC blocker above is not accepted as a physical conditioning failure. `loaded_dc` incorrectly sources electrode states from `basis.voltage`; `include_solution` later subtracts trained bubble modes from its component base columns. The original cardinal electrode states remain in `basis.harmonic` and must be used for both the conductance matrix and full loaded DC potential. The 20 focused tests and typecheck were independently repeated, but they did not cover this post-training electrode-state distinction. A bounded correction and new cold control are therefore justified without changing the LU guard.

## 7. Parent-directed cardinal-harmonic correction: before/after

This addendum starts from checkpoint `e06a7523003b1d79972b3a1847fdceac33cfa9fe`. It preserves sections 1–6 as the before-history. No commit or push was made, and no further numerical attempt is authorized by this run.

### Exact correction and regression

- `Basis::harmonic_electrodes()` now exposes the immutable prefix of private `basis.harmonic` selected by `electrode_dofs`; trained bubbles continue to modify only `basis.voltage`.
- `loaded_dc` now uses those cardinal harmonic electrode lifts both to assemble electrode conductance and to reconstruct the full loaded DC potential. Reduced potential/current reconstruction remains on `basis.voltage`/`basis.current`. The LU guard, equilibration, solver, PDE, mesh, reduction, and physical formulation are unchanged.
- The v2 diagnostic packet additionally records `cardinalHarmonicVoltageBasis`, so the independent audit can replay the same source-of-state choice rather than infer it from trained coordinates.
- The new `trained_bubbles_change_voltage_coordinates_but_not_cardinal_harmonic_electrodes` regression uses a center-fan mesh with two interior DOFs. It inserts a manufactured solution, verifies that a bubble was added and `basis.voltage[0]` changed, and verifies that the cardinal harmonic values and contact cardinality remained unchanged.
- The audit, plot, and cold supervisor accept explicit attempt stems. The new requests are byte-identical copies of the old fixed requests; old artifacts were not overwritten.

### Focused checks and builds

```sh
PATH="/c/Users/kiril/.cargo/bin:$PATH" cargo test --manifest-path native/analyze/Cargo.toml --test conservative_current --test board_response --test board_prepare --test volume_projection
python -m py_compile debugging/p7-d1-loaded-current/run_cold.py debugging/p7-d1-loaded-current/audit.py debugging/p7-d1-loaded-current/plot.py
PATH="/c/Users/kiril/.cargo/bin:$PATH" npm run native:build
PATH="/c/Users/kiril/.cargo/bin:$PATH" cargo build --release --manifest-path native/analyze/Cargo.toml --example conservative_current
npm run typecheck
git diff --check
```

| Check | Result | Command wall time |
| --- | --- | ---: |
| Focused Rust tests | PASS: 22/22 = 5 conservative + 9 response + 6 prepare + 2 volume projection | 2.341 s |
| Python syntax | PASS for supervisor, independent audit and plot | PASS |
| First native build | Tool timeout during final addon compilation; compilation continued and no code error was reported | 300.557 s |
| Incremental native build | PASS | 1.664 s |
| Release cold-runner build | PASS | 59.692 s |
| TypeScript typecheck | PASS | 5.241 s |
| Final `git diff --check` | PASS; LF-to-CRLF warnings only | PASS |

| Item | SHA256 |
| --- | --- |
| New addon `eda-copilot-node.win32-x64-msvc.node` | `60a9d1d233e34a92e6da5dc30b08b60ebcd7e05399957d1f70badd561a1fbdc7` |
| New cold runner `conservative_current.exe` | `2fe2d9789c804d029981d3b56b694d84658c4a97b26c3b58946064ffd93d5ba6` |
| `volume_reduction.rs` | `7eb2aee7a0c51d7352eb898c62ecf33da0bcf1bd548e0a42f75c10dde2e19209` |
| `board_prepare.rs` | `00039b421d9fdd9b5cacaf9012c2a75d84941db0d2def6e61368b3d01a3ea089` |
| `volume_projection.rs` regression | `5feced9d4e81eaebde162e9add581fe3dde39d0e3fed176a1eee727293c5a1c6` |
| Cold supervisor | `5a635e18e2975d0306663f8ff925b08e29a0be530477023c2e1480b637539e98` |
| Independent audit | `58f8c10fee68991d9fdc67af0a63aef8e34cc77a54bc34b82f69ef52eab9d1b6` |
| Plot script | `704f83ec0ea2704f43d36feda36f6dc3711ef463915ee00868a1cac5360ef941` |
| Slot/continuous harmonic requests | `fdbc0bcc1152ed859025ef4044ea6d14c30dc7cb9c68105751122a7632885840` each |

### New cold slot and actual after-result

`python debugging/p7-d1-loaded-current/run_cold.py slot harmonic` started at `2026-10-31T18:02:00Z`, with no cache/checkpoint, one thread, a 300 s deadline, a 2 GiB private cap and a 1 GiB host commit reserve. It completed all four full/reduced 1 kHz unit-port states, then the loaded DC solve was again rejected by the unchanged LU guard:

```text
NETWORK_SINGULAR: nonfinite/rank deficient or ill-conditioned system; condition=111045687460217.7
```

| Quantity | Observed |
| --- | --- |
| Status / exit | `child-failure` / 1 |
| Wall time | `27.797 s` |
| Peak sampled RSS | `414314496 B` (`395.12 MiB`) |
| Peak private commit | `411058176 B` (`392.02 MiB`) |
| Initial commit headroom | `21965770752 B` |
| Initial available physical memory | `5793742848 B` |
| Resource/time bounds | PASS |
| Four full/reduced 1 kHz unit states | PASS; `developerVerification 17.422501 s` |
| Loaded DC LU admission | **NO-GO: `1.110456874602177e14 > 1e12`** |
| `slot-harmonic-full.json` | Not created; output SHA256 null |
| Continuous harmonic | NOT RUN after slot failure |

The corrected source selection therefore did not remove the real-fixture LU refusal. The identical condition estimate is an observed result, not evidence that the correction was absent: the focused regression and binary/source hashes above distinguish the new attempt. This addendum does not reclassify the remaining refusal as a proven physical blocker or authorize changing its guard.

Because no loaded-state output exists, the independent numerical audit could not run. No `slot-harmonic-audit.json`, continuous audit, numerical plot, complex DC/1 kHz BOTTOM/TOP/all-conductor flux, return split, section areas, algebraic residual, or full/reduced observable error was produced. The old failed slot evidence remains intact in `slot-cold.log` and `slot-supervision.json`; the new evidence is separately stored in `slot-harmonic-cold.log` and `slot-harmonic-supervision.json`.

### Addendum gate ledger

| Gate | Result |
| --- | --- |
| Immutable cardinal harmonic source for full loaded DC | PASS by implementation and focused regression |
| Reduced reconstruction remains on trained bases | PASS; unchanged |
| LU/equilibration/solver/physical formulation unchanged | PASS |
| Focused tests, builds, typecheck, Python syntax, diff | PASS |
| New cold slot time/resource bounds | PASS |
| Loaded DC LU admission | **NO-GO** |
| Independent replay and algebraic gates `1e-8` | NOT RUN / NOT PRODUCED: no output JSON |
| DC section balance and full/reduced `2%`, floor `1e-6 A` | NOT PRODUCED |
| Flux tables and plot | NOT PRODUCED |
| Continuous control | NOT RUN |
| Current-conservation PASS / conditional joint reference | **NO-GO / not authorized** |
| Separate magnetic patch `10.2046289% > 2%` | Unchanged independent blocker |

Stop for parent review. No commit, push, continuous run, repeated slot, audit/plot fabrication, hybrid, STM32, direct DSL, magnetic/domain refinement, or further solver change was made.

## 8. Global DC voltage gauge addendum: before/after

The parent proved from the saved harmonic matrix that the remaining near-zero eigenvalue is the common six-electrode voltage gauge and that the cardinal states were reconstructed accurately. Sections 1–7 remain unchanged as the prior failure history. This follow-up still starts from HEAD `e06a7523003b1d79972b3a1847fdceac33cfa9fe`; no commit or push was made.

### Numerical gauge and anchor-invariance checks

For the connected loaded network, retain the original full matrix and RHS

```text
M = G + B*diag(1/R_p)*B^T,       r = B*d.
```

For one numerical anchor `a`, set `V_a=0`, exclude only its row/column, and solve `M_FF V_F=r_F` through the existing bounded complex LU. No physical ground, resistor, equilibration, guard relaxation, or PDE change is introduced. The reconstructed vector is checked against every original row of `M V=r`; Norton balance is evaluated at each positive terminal as `d_p - V_p/R_p - (G V)_a`. This is a validation formula, not a change to the loaded network.

The production path solves with anchor electrode 0 and independently with anchor electrode 1. It requires both full residuals, both Norton residuals, and the port-voltage/full-current anchor differences to be at most `1e-8` before RT0 recovery. The DC packet records these gauge diagnostics. Reduced reconstruction and all AC/RLC/loss/magnetic paths remain unchanged.

The manufactured regression uses a four-electrode connected conductance Laplacian with unequal branch conductances, two disjoint ports, and nonzero drive `[0.8,-0.35] A`. It verifies the constant raw-conductance nullspace, zero potentials at anchors 0 and 3 respectively, full residual/Norton balance, and matching port/electrode-current results under the two gauges.

The independent Python audit now repeats the row/column elimination for anchors 0 and 1, recomputes full residual/Norton/port/full-current invariance, replays the saved gauge diagnostics, and adds a separate `dcVoltageGaugeFullResidualNortonAnchorInvariance1e-8` gate. It was syntax-checked but has no real output to audit after the new refusal.

### Tests, builds, and identity

| Check | Result | Command wall time |
| --- | --- | ---: |
| Focused Rust tests | PASS: 23/23 = 6 conservative + 9 response + 6 prepare + 2 volume projection | 3.106 s |
| Python supervisor/audit/plot syntax | PASS | PASS |
| First native build | Tool timeout during final addon compilation; no code error | 300.529 s |
| Incremental native build | PASS | 1.671 s |
| Release cold-runner build | PASS | 59.895 s |
| TypeScript typecheck | PASS | 5.218 s |
| `git diff --check` | PASS; LF-to-CRLF warnings only | PASS |

| Item | SHA256 |
| --- | --- |
| Addon `eda-copilot-node.win32-x64-msvc.node` | `073d26950bc171f63b4df01c1fa8f13299abd356813eef1d111bf0e40f3c0d9f` |
| Runner `conservative_current.exe` | `44d461e8206ad25a4c0052920509aa1d47b97bc6fefcf7b992e595bfd5ce9196` |
| `board_prepare.rs` | `9c15791ca0823347b52bd1c6409f61d4cd5c93e21c5503b58baae62a34c343e2` |
| `board_prepare.rs` focused test | `99bc6c31e2a8cd92d9a1379879ea830e60c3926afa5054407a98c6426c1887a7` |
| Independent audit | `11f843f9a183b40c2f1f94c95da7210756e9eab9478909e3684a70341553f5f6` |
| Gauge slot request | `fdbc0bcc1152ed859025ef4044ea6d14c30dc7cb9c68105751122a7632885840` |
| `slot-gauge-supervision.json` | `4c0fb518bb1de91a1c279eeb131bc31cb83a350016fbdb86dcb09814fa2f5376` |
| `slot-gauge-cold.log` | `1c1c059e8d698b0ef761ce1fff5752a09430521cb326aa76c4c0e10056f958cd` |

### Sole `slot-gauge` control and next refusal

`python debugging/p7-d1-loaded-current/run_cold.py slot gauge` started at `2026-10-31T18:23:22Z` with the unchanged board/request, no cache/checkpoint, one thread, a 300 s deadline, a 2 GiB private cap and a 1 GiB host commit reserve.

| Quantity | Observed |
| --- | --- |
| Status / exit | `child-failure` / 1 |
| Wall time | `28.172 s` |
| Peak sampled RSS | `414474240 B` |
| Peak private commit | `411086848 B` |
| Initial commit headroom | `21945008128 B` |
| Initial available physical memory | `5767761920 B` |
| Resource/time bounds | PASS |
| Four full/reduced 1 kHz unit states | PASS; `developerVerification 17.667252 s` |
| Anchored loaded DC LU, full residual/Norton and two-anchor checks | PASS by reaching the subsequent RT0 recovery; numeric diagnostics were not serialized |
| Final loaded DC conservative recovery | **NO-GO: `LOCAL_CONSERVATIVE_RESIDUAL: nodal=1e0 cell=1e0`** |
| `slot-gauge-full.json` | Not created; output SHA256 null |
| Continuous gauge | NOT RUN after slot failure |

Thus the former `1.110456874602177e14` LU refusal is no longer the current blocker. The next refusal occurs while recovering the final full or reduced loaded DC current; the log does not distinguish which side, and no additional run was used to refine that diagnostic. No loaded state, independent numerical audit, plot, complex flux/return table, section area, or full/reduced observable error was produced.

### Gauge gate ledger

| Gate | Result |
| --- | --- |
| Single global numerical voltage gauge, no physical ground | PASS |
| Full original residual, Norton and anchor invariance `1e-8` | PASS in manufactured regression and by cold execution reaching recovery |
| Focused tests/builds/typecheck/Python syntax/diff | PASS |
| Cold slot resource/time bounds | PASS |
| Previous loaded DC LU condition refusal | Cleared in this attempt |
| Final loaded DC RT0 nodal/cell residual | **NO-GO: normalized refusal `1e0` / `1e0`** |
| Independent loaded-state replay and section gates | NOT RUN / NOT PRODUCED: no output JSON |
| Full/reduced `2%`, floor `1e-6 A`; flux tables and plots | NOT PRODUCED |
| Continuous control | NOT RUN |
| Current-conservation PASS / conditional joint reference | **NO-GO / not authorized** |
| Separate magnetic patch `10.2046289% > 2%` | Unchanged independent blocker |

Stop for parent review after this single next refusal. No repeated slot, continuous run, further solver/recovery change, hybrid, STM32, direct DSL, magnetic/domain refinement, commit, or push was made.

## 9. Saved-state RT0 and AC RHS correction: actual numerical result

This addendum preserves sections 1–8 as the failure history. It starts from the same uncommitted gauge work at HEAD `e06a7523003b1d79972b3a1847fdceac33cfa9fe`. The new result contains real loaded-state output and an independent audit, but **D1 remains NO-GO**. Continuous was not run.

### 9.1 Saved-state diagnosis and bounded implementation changes

No new FEM solve was used to diagnose the gauge slot's `nodal=1 cell=1` refusal. `parent_rt0_replay.py` loaded its saved matrix and targets: `C C^T` is `189×189`, has condition `3994.752558496488`, and both saved loaded DC targets are compatible. The refusal came from relative-only normalization of numerically zero reaction/divergence rows, not from a loaded-state incompatibility.

`conservative_current.rs` now uses the mixed denominator `max(localScale,targetScale)` with the unchanged `1e-8` threshold. A row can therefore pass either its local relative test or the absolute component at the current state's scale; a finite incompatible current is not excused. On the saved data the replay residuals are:

| State | Nodal normalized residual | Cell normalized residual | Result |
| --- | ---: | ---: | --- |
| Full loaded DC | `2.5307909580678008e-12` | `6.7204021337427166e-12` | PASS |
| Reduced loaded DC | `6.958324371958175e-13` | `1.0623159128317912e-12` | PASS |

The new `dc_roundoff_at_zero_reaction_and_divergence_is_not_a_false_residual` regression covers this roundoff case. The previous manufactured incompatible-mode control, with a real approximately `0.416 A` violation, remains in place. Recovery errors now identify full/reduced loaded DC/AC real/imaginary context.

The following `slot-roundoff` passed DC but exposed an independent implementation bug in AC superposition: the complex RHS had shape `1×np` instead of `np×1`, causing `index out of bounds: the len is 1 but the index is 1` in `frequency/math.rs`. `loaded_ac_coefficients(...)` now constructs one `np×1` complex RHS, validates square/finite inputs, and has a direct nonzero-imaginary regression. No PDE, mesh, reduction, LU threshold, physical load or accuracy gate was changed.

### 9.2 Final checks and cold control

| Check | Result |
| --- | --- |
| Focused integration tests | PASS: 24/24 = 7 conservative + 9 response + 6 prepare + 2 volume projection |
| AC RHS unit test | PASS: 1/1 |
| Native addon build | PASS |
| Release cold-runner build | PASS |
| TypeScript typecheck | PASS |
| `git diff --check` | PASS |

`python debugging/p7-d1-loaded-current/run_cold.py slot ac-rhs` started at `2026-10-31T18:51:37Z` with the unchanged board/request, no cache/checkpoint, one thread, a 300 s deadline, a 2 GiB private cap and a 1 GiB host commit reserve.

| Quantity | Observed |
| --- | ---: |
| Native status / exit | PASS / 0 |
| Wall time | `38.35999999998603 s` |
| Peak sampled RSS | `414404608 B` |
| Peak private commit | `411148288 B` |
| Initial commit headroom | `21922467840 B` |
| Initial available physical memory | `2662342656 B` |
| Resource/time bounds | PASS |
| `slot-ac-rhs-full.json` | Created; native loaded DC and 1 kHz states available |
| Continuous | NOT RUN because the subsequent audit is NO-GO |

| Item | SHA256 |
| --- | --- |
| Cold runner | `cae0c120f084cefa34e5863eaccd7a7d290b81d7fe03522fc8f75065de8b3e80` |
| Full output | `483aab4d3fd449bb2a480369f4b566e7a9316c535427135bc0ff1881ef327318` |
| Independent audit JSON | `b66967ec1504b0528317780902ea1a8e2d14cdd4fa42effd2bbb2d6ef0f33d31` |
| Audit plot | `7a877d5ac0270a35ab9631c4f186436791241bbcaced4b8c97636e49bc295d21` |

### 9.3 Independent-audit corrections

The audit itself required corrections before its result could be interpreted:

- support NumPy boolean serialization and use the same mixed nodal/cell residual normalization as production;
- check DC electrode signs against physical outward face flux, align reduced DC potentials only by a per-component constant gauge, and do not compare gauge-dependent `reducedState` coordinates as physical voltages;
- anchor the independent dense DC solve by eliminating the first potential DOF, avoiding an unanchored NumPy result with nonphysical potentials around `4.09e17 V`;
- compare near-zero serialized DC diagnostics by absolute error;
- do not equate AC physical electrode face conduction with raw nodal reaction or Norton input: charge and contact-adjacent cells contribute. The former incorrect `acElectrodeNortonBalance1e-8` gate was removed, while `electrodeSourceDifferenceIncludingContactCharge` is retained as a diagnostic rather than hidden or treated as an accepted balance.

These changes affect verification only. They do not weaken the production RT0 solve or turn AC charge/displacement current into a failed conduction balance.

### 9.4 Replay, algebraic and full/reduced results

DC exact replay passes with maximum normalized error `1.8578640848112896e-9`. The two-anchor DC diagnostics also replay within `3.16745910245098e-9` absolute error. DC algebraic continuity, electrode totals, section/electrode balance and full/reduced observables pass.

At 1 kHz, full/reduced nodal and cell continuity remains at machine precision:

| Constraint | Full | Reduced |
| --- | ---: | ---: |
| Nodal reaction normalized error | `3.8266885039907204e-16` | `2.5511260010210907e-16` |
| Cell divergence normalized error | `6.837394465556514e-16` | `1.2390608762420184e-15` |

However, exact independent loaded-state replay fails with maximum normalized error `0.045126035759688383`. Principal component errors are reduced imaginary port voltage `0.045126035759688383`, full imaginary potential `0.034396696002636645`, reduced imaginary potential `0.03729130484478593`, full imaginary current `1.6765866914015532e-05`, reduced imaginary current `1.6936846818626277e-05`, full port voltage `6.056707841201777e-06`, and `reducedState` `4.4326158996619757e-07`. The real parts replay closely; the unresolved mismatch is concentrated in the AC imaginary state. Its cause is not yet localized and no PASS is inferred from the small real-part errors.

| Observable | DC full/reduced max error | 1 kHz full/reduced max error |
| --- | ---: | ---: |
| Cell current | `2.1559455032433342e-08` | `3.5724972066572507` |
| Face current | `1.0169674605774485e-08` | `12.670476909468485` |
| Section flux | `8.548717289613705e-07` | `0.007288560550462076` (`0.728856%`) |
| Port | `1.0756381321529666e-09` | `2.6484861157036874e-05` |

Thus DC passes the `2%` gate with floor `1e-6 A`; AC sections and ports are close, but the strict local cell/face comparison fails. The good section result does not override that gate or the independent replay failure.

### 9.5 Complex section flux and areas

Entries below are RMS complex amplitudes `(real, imag)` in A; DC imaginary parts are zero. Full/reduced values use the same fixed sections and `+x` normal. Areas are identical for full and reduced because they are geometric properties.

| State | x (mm) | TOP full | TOP reduced | BOTTOM full | BOTTOM reduced | All full | All reduced |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| DC | 0.500123 | `0.999554431488` | `0.999554431489` | `-0.999554431489` | `-0.999554431488` | `-3.5567e-13` | `4.9618e-13` |
| DC | 1.000123 | `0.999554431488` | `0.999554431489` | `-0.999554431489` | `-0.999554431488` | `-3.6099e-13` | `4.9388e-13` |
| DC | 1.500123 | `0.999554431489` | `0.999554431490` | `-0.999554431489` | `-0.999554431488` | `5.1623e-13` | `1.2400e-12` |
| 1 kHz | 0.500123 | `(1.032149916112, -1.7891e-7)` | `(1.032149917931, 6.2052e-6)` | `(-1.086468000427, 1.7128e-6)` | `(-1.086468001759, -1.0039e-6)` | `(-0.054318084314, 1.5339e-6)` | `(-0.054318083828, 5.2012e-6)` |
| 1 kHz | 1.000123 | `(0.968465658588, 2.7280e-6)` | `(0.968465651703, -3.2863e-5)` | `(-0.978280207556, -8.7664e-6)` | `(-0.978280205847, -4.4709e-5)` | `(-0.009814548969, -6.0383e-6)` | `(-0.009814554144, -7.7572e-5)` |
| 1 kHz | 1.500123 | `(1.060775162538, 1.6111e-6)` | `(1.060775163567, 8.1341e-6)` | `(-0.988170601423, -1.0195e-5)` | `(-0.988170599505, 1.0364e-5)` | `(0.072604561116, -8.5842e-6)` | `(0.072604564061, 1.8499e-5)` |

| x (mm) | BOTTOM area (m²) | All-conductor area (m²) | BOTTOM cells | Unknown-layer cells |
| ---: | ---: | ---: | ---: | ---: |
| 0.500123 | `5.000000000000001e-09` | `6.99999988079071e-09` | 39 | 0 |
| 1.000123 | `1.9000000506639473e-09` | `3.89999993145466e-09` | 12 | 0 |
| 1.500123 | `4.999999999999999e-09` | `6.99999988079071e-09` | 33 | 0 |

The maximum normalized DC section balance error is `5.181499122726202e-13`. The generated and opened plot is `debugging/p7-d1-loaded-current/conservative-current-slot-ac-rhs-audit.png`; it shows DC return-current redistribution, AC charge/displacement contributions, and the primarily imaginary full/reduced discrepancy without empirical scaling.

### 9.6 Final gate ledger and next bounded step

| Gate | Result |
| --- | --- |
| Saved loaded DC targets compatible; zero-row false refusal removed | PASS |
| AC complex RHS orientation and unit regression | PASS |
| Focused tests/builds/typecheck/diff | PASS |
| Cold slot native completion and resource bounds | PASS |
| Algebraic continuity and DC electrode `1e-8` | PASS |
| DC gauge/full residual/Norton/two-anchor invariance `1e-8` | PASS |
| DC section/electrode balance `1e-8` | PASS |
| Exact loaded-state independent replay `1e-8` | **NO-GO at 1 kHz: `0.045126035759688383`** |
| Full/reduced observables `2%`, floor `1e-6 A` | **NO-GO for local 1 kHz cell/face currents; sections pass at `0.728856%`** |
| Continuous control | NOT RUN after slot audit NO-GO |
| Current-conservation PASS / conditional joint reference candidate | **NO-GO / not authorized** |
| Separate magnetic patch `10.2046289% > 2%` | Unchanged independent blocker |
| D1/P7/P8/P11 acceptance | Not claimed |

The next bounded diagnostic should localize the AC imaginary replay and local full/reduced mismatch using `slot-ac-rhs-full.json` and the independent replay, without a new FEM run. This is not yet a general-purpose extractor result and does not close P7.
