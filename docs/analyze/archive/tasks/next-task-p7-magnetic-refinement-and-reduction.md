# P7: corrected mixed precision, then a smaller magnetic system

Status: assigned; implementation and measurements required, not an audit-only task.

This supersedes the scope of next-task-p7-magnetic-f32.md. Read p7-magnetic-f32-results.md including parent review, p7-memory-results.md and p7-end-to-end-budget-results.md. Work sequentially so numerical and discretization errors remain distinguishable. The default remains the accepted F64 path until parent review.

## A. F32 factor with F64 residual correction

- Recover the rejected experiment from debugging/p7-magnetic-f32/rejected-runtime.patch selectively; do not restore unrelated experiments. Keep geometry, matrix assembly, original coefficients, reductions and physical outputs F64; only the magnetic factor and triangular solves use F32. Capacitance and other shared-helper clients remain F64.
- Implement bounded iterative refinement using the original F64 matrix residual and the same F32 factor for corrections. Apply it to the nine exterior-enrichment right-hand sides BEFORE constructing the Schur matrix, and to other magnetic solves where required. This is correction of the existing direct solve, not a new Krylov solver.
- Preserve RHS, handle triangular matrix storage correctly, check finite values, per-RHS backward residual, stagnation and iteration exhaustion. Use an explicit small finite iteration cap (at most 8), shared cancellation/deadline and accounted scratch. Do not allocate a second full F64 factor, regularize, symmetrize away a failed reciprocity check, silently fall back or relax existing guards.
- Log initial/final residuals, correction count and normalized Schur antisymmetry including on failure. Existing strict reciprocity/passivity/convergence checks must pass. A physical 8% allowance does not replace numerical correctness.
- Test identical coefficients/RHS for F64 and corrected F32, including multiple RHS and failure handling; run independent frozen magnetic energy fixture. If admitted, run small complete ordinary DSL against the saved refined whole reference. Record completed-run memory and stage times, not successful-versus-early-failed timings.
- Own TS precision cache identity, provenance and executor freeze plus tests; parent will not implement these in parallel. Keep default F64, reject invalid precision values, prevent F32/F64 cache collisions.

## B. Implement reduction of the magnetic system

Proceed after A has a recorded pass or rejection. Failure of F32 must NOT terminate this task: perform B on F64 if necessary.

- Measure actual unknown categories on the real STM32 request without first performing an expensive factorization. Distinguish counts from factor-fill costs; do not extrapolate the tiny fixture's proportions.
- Implement ONE conforming reduction of far-air approximation while preserving copper, bores, ports, near-copper resolution, source space, exterior domain and coupled interactions. Prefer a conservative far-air low-order patch with explicit compatible interface constraints and local detail retained at its boundary. Existing shared-entity minimum-degree logic must not accidentally lower protected copper/near-field traces. Compact airP2 has no cell-private modes to remove: merely deleting nonexistent interior modes is not an implementation.
- Select the minimal viable conforming construction from the current basis representation, document it before coding, and implement it. No broad method sweep, new meshing dependency, iterative solver, GPU, domain truncation or omitted magnetic coupling. If the preferred construction is incompatible, document the concrete obstruction and choose one minimal conforming alternative, not repeated unrelated experiments.
- Verify conformity across interfaces, protected basis/geometry retention, symmetric positive energy, unchanged source/port mapping and independent physical accuracy. Add a regression exposing accidental shared-trace reduction.
- First compare original F64 vs reduced-system F64. Only if A and B independently pass, compare the combined reduced-system corrected-F32 result to the ORIGINAL physical reference. Errors may not be summed and treated as individually acceptable; final delay/loss/S and independent energy must meet the agreed 8% physical allowance with reference uncertainty accounted for.

## C. Bounded real evidence and cleanup

- Before full numeric allocation, report unknowns, matrix nnz, factor fill/values/scratch and live conversion buffers. Avoid expensive full solves for candidates already failing small controls or showing no meaningful memory gain.
- For the selected passing candidate, one supervised real STM32 complete DSL cold/warm/restart attempt. Existing supervisor, at most 8GiB tree private commit and 600s per cold call, host commit headroom checked; OS paging opt-in permitted, no pagefile changes. Goals <=4GiB and <=120s cold, <=5s warm, <=10s restart remain goals, not grounds to hide failure. No unchanged heavy retries. Report separately actual peak RSS/private commit, estimates, stage times, cancellation/resource failures and cache hits.
- Keep one accepted implementation path and only the necessary controlled baseline for comparisons. Remove failed runtime branches, preserve evidence under ignored debugging and preserve user native/analyze/debugging. No git clean/reset, deleting fixtures or modifying reference values.
- Run normal npm run native:build for retained Rust changes, focused Rust numerical/interface tests, TS cache tests, typecheck/build when TS changes. Update CHANGELOG and p7-magnetic-refinement-and-reduction-results.md with commands, binary/request/profile identities, before/after tables, acceptance ledger and actual result visualization for successful DSL runs.
- No commits or push until parent review. Do not claim complete P7 closure from memory controls alone: list remaining original real-channel and bandwidth obligations.

## Handoff

Deliver working code for both stages where valid, not just proposals. If a stage fails, include measured reason, remove its failed runtime path and still complete the independent other stage. State exactly what reduced memory, by how much, the final accuracy, and whether the real full DSL completed. Keep updates concise and avoid unnecessary status polling or repeated runs.
