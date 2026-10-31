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
