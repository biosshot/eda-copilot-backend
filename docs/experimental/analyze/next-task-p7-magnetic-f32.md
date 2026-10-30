# P7: bounded magnetic F32 experiment and unknown-count audit

Status: initial experiment finished and rejected; see p7-magnetic-f32-results.md. Follow-up implementation is assigned in next-task-p7-magnetic-refinement-and-reduction.md. Preserve production F64 default until evidence is reviewed.

## Scope

Reduce magnetic memory without changing the physical domain, copper, ports, mesh, ordering or electrical/thermal solvers. First test F32 numeric magnetic Cholesky factors and triangular solves. Keep assembly, original matrix residual evaluation, accumulation, reduced models and public transport in F64. This is explicitly mixed precision, not a full F32 migration. The shared sparse potential helper also serves capacitance: keep that path unchanged.

Use the existing direct solver and existing bounded correction mechanism. No new iterative solver, regularization, silent magnetic omission, out-of-core implementation or dependency. Preserve existing residual, reciprocity, passivity and convergence guards; 8% physical error is not permission to relax linear-system correctness. If F32 cannot pass, report and remove the failed runtime experiment.

## Work and evidence

1. Inspect existing user changes and preserve native/analyze/debugging. Record an unchanged F64 baseline and exact profile/request identity. No repository-wide F64 replacement.
2. Add one explicit developer-only precision selection, default F64. Include precision in cache identity/provenance and reject within-executor changes. Account real scalar widths, indices, conversion buffers, scratch and retained F64 matrix. Never retain both full F64 and F32 factors.
3. Compare F32/F64 on the same coefficients/RHS, independent magnetic-energy fixture and full small mixed-channel DSL. Report failures as failures. Use saved independent refined reference; final sampled delay/loss/S error <=8% and strict existing numerical guards are required. Separate rounding error from the existing geometric error.
4. Only after controls pass, one bounded real STM32 symbolic memory plan and, if host commit headroom admits, full magnetic/DSL attempt. Use existing OS-paging supervisor, a finite 600s ceiling, <=8GiB process-tree private commit cap; do not change OS pagefile. Report actual RSS/private commit plus estimates separately. No repeated long runs after unchanged failure. Target <=4GiB / <=120s remains unproven until measured, not imposed by silently truncating physics.
5. Record unknown counts and matrix/factor memory by air, near-copper trace and cell-private basis where actually measurable. Explain the smallest conforming local-coarsening option. Do not implement a second approximation simultaneously with the precision experiment.
6. Normal native build for retained Rust changes, focused numerical/cache tests, typecheck/build for TS changes; update CHANGELOG and a concise p7-magnetic-f32-results.md. Preserve physical references, remove failed experimental runtime branches, no commit/push without parent review.

## Completion

Provide measured cold/warm/restart when a full DSL succeeds, matrix identity, peak memory and stage times, accuracy and numerical residuals. State whether F32 is acceptable, conditional or rejected. P7 remains open unless all original real-channel, bandwidth and physical acceptance obligations are met; this experiment alone does not close it.
