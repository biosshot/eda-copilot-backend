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

### Subsequent cleanup

The initial inventory above is superseded for the following paths: tensor
potential/operator and sparse iterative modules have now been removed. Their
pre-cleanup sources remain in ignored audit evidence. The direct FEM operator
retains direct factorization only. Tensor candidate controls were removed;
independent frozen data and the direct FEM comparison remain.

Integral/surface/cache operators moved to `native/analyze/tests/reference/local`;
the production library no longer exports or compiles them. Reference examples and
tests load them explicitly from that tree. Shared arithmetic and geometry remain
in place, with read-only quadrature access and the existing tetra walker exposed
for reference reuse. The board diagnostic now accepts only direct FEM entity
configuration and geometry/field/full/train modes, rejecting removed dispatch
options before reading/building the board. This still does not provide a worker
entry point.

`cargo check --manifest-path native/analyze/Cargo.toml --examples --test volume_integral --test volume_galerkin`
passes after these edits. The unchanged frozen three-tetra direct FEM screen
passes in 61.83 s (debug test): numerical energy error 0.0030946518528586044 plus
inherited empirical reference uncertainty 0.006780636440064506, total
0.00987528829292311. This is a primitive reference check, not real-board 5%
acceptance or cold production performance.

The full-cell matrix cache is now reference-only too. The diagnostic uses the
existing sparse `Field`/`ElectricApply` directly and requests only needed RHS;
it no longer materializes full C as a prerequisite. Complete checkpoint loading
remains an explicit diagnostic option. The original exact 17 frequencies are
copied from the accepted current-space fixture; full diagnostic checks now
schedule both ports individually and opposite drive on each frequency.

The nested P6 512 MiB/110 s request has been replaced by remaining shared bytes
after conservative live-operator/basis/training reservations and remaining time
from one 120 s start. Builder/solve calls receive remaining time. Sparse factor
assembly itself still needs cooperative deadline propagation; this change alone
does not certify the entire lifetime cap. A supervised actual cold run is still
required.

Final `cargo check` of examples/reference tests passes after the reference moves.
The original-pair checkpoint identity/corruption regression passes (0.11 s).
Ordinary `npm run native:build` passed in 9m00s, separately from calculation
timing, before the final matrix-cache export removal; a final runtime acceptance
build and DSL matrix remain required.

### Bounded actual-ROI baseline: refused

The one post-cleanup baseline used the unchanged real STM32 board/request and
`entity:6:8:16:3 train`, after a normal standalone release build (3m45s, no
opt-level override). It refused the supervisor wall cap at **120.0989665 s**,
peak family RSS **1025327104 bytes** (977.83 MiB), exit 1. The executable SHA256
is `55abba4ea173e5e0d41bd228d523fda11d423cf129d1cd8aaa3159cfc330c984`.
Details are preserved in
`debugging/p7-board-integration/single-method-requested-rhs-v1/direct-fem/`.

Three training solves completed (7, 7, 12 iterations); maximum observed KCL was
2.11077e-16, KVL 3.93711e-12, complex-power error 2.55990e-16 and Joule-power
error 2.08486e-15. These are equation audits, not physical observable accuracy.
No completed model/holdout output, warm run, regional-power table or physical
5% acceptance exists. Requested-RHS storage alone did not resolve the cold-time
failure. No refined run or parameter search followed this refusal, in accordance
with the bounded assignment's stopping rule.

The immutable request and predeclared observable/floor plan are saved as
`tests/fixtures/analysis-local-extraction/board-integration/single-method-via-request.json`
and `single-method-baseline-plan.json`. Their source request hash is unchanged.
Reproduction command (run under one 120s/2GiB family supervisor, not bare for
resource acceptance):

```powershell
$env:CARGO_BUILD_JOBS='1'
cargo build --release --manifest-path native/analyze/Cargo.toml --example board_volume
native/analyze/target/release/examples/board_volume.exe tests/pcb-analyze/STM32G0_CAN_RS485/STM32G0_CAN_RS485.json tests/fixtures/analysis-local-extraction/board-integration/single-method-via-request.json entity:6:8:16:3 train
```

The first real worker/direct DSL result and the remaining P7 board/loss/PDN
matrix are therefore still unimplemented, not accepted or silently skipped.

Post-cleanup focused validation passed: 10 local-library unit tests, 3 exact
field-refinement tests, 8 current/geometry tests and 7 field/replay tests, in
addition to the direct frozen screen and reference-cache test above. Together
with the initial 26 P6 integration tests this is 56 passing focused tests.
The cold-run JSON is also retained as the small
`single-method-cold-run.json` fixture; no binaries/build outputs were staged.
Viewed the generated diagnostic comparison
`debugging/p7-single-method-audit/cold-run-comparison.png`; it labels both cold
timeouts explicitly and makes no accepted current/loss/frequency claim.

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
