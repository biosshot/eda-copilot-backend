# P7 single-method audit checkpoint — 2026-10-28

This is an inventory checkpoint, not completion of the single-method assignment.
The selected local candidate is the existing direct sparse FEM. No physical
acceptance or successful board/worker/DSL extraction is claimed.

## Preserved evidence

Initial tracked/untracked inventory and dependency search are saved in ignored
`debugging/p7-single-method-audit/`. No user debugging files were removed.
The two existing `review-2gib-comparison-v1/*/run.json` files were read; expensive
successful references were not regenerated.

| Existing cold diagnostic | Wall seconds | Peak family RSS bytes | Result |
| --- | ---: | ---: | --- |
| Direct FEM, entity:6:8:16:3 | 120.0787639 | 1025654784 | Supervisor wall refusal; no complete holdout |
| Tensor, tensor:6:8:16 | 111.4839997 | 94924800 | Exit 1, internal deadline; not a supported candidate |

Neither provides the required 17-frequency observable error table, successful
warm-cache performance, or physical acceptance. Same-grid independent C agreement
is numerical verification only. The historical full-C convergence failure is not
automatically an observable failure under the new 5% policy; those observables
still require their own declared absolute tolerances and combined error budget.

## Dependency inventory before deletion

`volume_magnetic` directly contains the integral-operator `MagneticApply` impl and
an optional `sparse_iterative::Preconditioner`/solve branch. `volume_tensor` uses
`tensor_potential`. `volume_integral` uses `volume_surface` and
`volume_integral_cache`; surface compensation and shared accurate/compensated
arithmetic have further dependencies. These cannot simply be deleted from
`local/mod.rs` while retaining the current callers.

Alternative paths are referenced by `board_volume`, `board_source_reference`,
`volume_magnetic_reference`, and `volume_galerkin`/`volume_integral` tests.
Shared prism/polyhedron functions also contain the text `volume_integral`; those
names are not proof of a dependency on the competing operator module. Accepted
current-space/near-far primitive code must be preserved.

No alternative source files have yet been deleted, moved, or hidden behind a
feature. No claim of a single production dispatch is made at this checkpoint.
`board_volume` remains a diagnostic example; its comment about production callers
does not establish a board-to-worker route.

## Completion matrix

| Obligation | State |
| --- | --- |
| Accepted P7a/P7b sections/RLGC and P6 | Preserved; existing P6 changes still need full fallback-specific acceptance |
| One direct FEM local backend | Selected, cleanup/integration incomplete |
| Fixed real via/return observables, combined error <=5% | Not demonstrated |
| Shared 2 GiB/120 s lifetime admission | Incomplete; nested P6 still requests 512 MiB/110 s |
| Worker and direct DSL via/return | Not delivered |
| Lines, transitions, PDN/reduction, supported loss matrix | Incomplete |
| STM32G0_CAN_RS485 real extraction DSL | No accepted result |
| TPS63060DSCR real extraction DSL | No accepted result |
| BandAmp real extraction DSL | No accepted result |
| Required final geometry/current/loss/frequency visualizations | Incomplete; historical diagnostic plots retained |
| Ordinary native build, typecheck, worker/direct DSL checks | Still required for runtime acceptance |
| P8/P11 extracted-board readiness | Not ready; neither stage implemented here |

P7 remains open. The working tree still contains accumulated P7 changes that
have not been accepted or committed by this inventory checkpoint. It must not be
reported as cleaned, and the untouched debugging directory must not be ignored
merely to make `git status` appear clean.

## Checks at this checkpoint

Before compilation, no cargo/rustc process was running. With
`CARGO_BUILD_JOBS=1`,
`cargo test --manifest-path native/analyze/Cargo.toml --test coupled --test structural_gauges --test physical_terminals`
passed all 26 tests (9/9/8). `npm run typecheck` passed. These verify existing
bounded P6 behavior, not the full fallback-specific 17-frequency/negative matrix
or new real-board observable acceptance. The compiler still reports the existing
unused import in capacitance and unused cross/sub helpers in mesh.
