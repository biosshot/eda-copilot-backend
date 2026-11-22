# Goal-bounded near/self omission

`P7_MAGNETIC_NEAR_BOUND=1` enables an experimental alternative to evaluating a cluster interaction when its entire possible contribution fits the existing action-error budget. Physical geometry, source ownership, current coordinates and requested tolerances are retained. The default is disabled.

## Bound and error accounting

For a tetrahedron of volume V with uniform absolute source density sigma, the maximum Newton potential at any point is bounded by `C sigma V^(2/3)`, where `C=2*pi*(3/(4*pi))^(2/3)<3`. A radially decreasing kernel has maximum integral on the equal-volume ball centered at the query point. Summing the individual tetrahedron bounds is valid even for duplicated or overlapping tetrahedra; no unverified disjointness assumption is used.

The planner uses the existing maximum normalized RHS envelope per original cell. Cube-root upper bounds are checked by outward interval cubing; unprovable/nonfinite bounds refuse omission and retain the ordinary traversal. Source potential bounds are summed through the tree. Both directed target contributions must fit the existing quarter source-mass allocation. Self blocks are handled once, not doubled.

Omitted potential bounds are multiplied by the original column goal and propagated through the existing potential-tail arrays to every original leaf, even without local expansion coefficients. The ordinary per-coordinate action bound and fresh original-current KVL checks remain in force. Omission is a distinct decision: it is not counted as an exact zero or a far expansion.

Certificates carry `nearBoundEnabled`, `nearBoundPolicy=summed-per-tetra-volume-v1`, and `boundedOmittedBlocks`. Sequential chunk totals preserve these fields, compact column certificates retain them, TS validation checks their consistency, and the response cache separates enabled and disabled policies. Default-off traversal/hash and potential-bound allocation fast paths are preserved.

## Verification

- Nonzero signed source currents on duplicate tetrahedra: omitted action encloses the unpruned result, including self and overlapping-source contributions.
- Cached/uncached omission action and bounds match exactly. Strong-source/tight-goal and nonfinite-potential controls refuse omission.
- An isolated opt-in two-chunk control reports two bounded omitted blocks, zero exact-zero blocks, and positive uncertainty under the original goals.
- Existing signed-action/full-KVL proof passes with the flag enabled. Compact certificate regression passes.
- Eight TS cache/validation tests and typecheck pass, including invalid omission provenance and mismatched chunk totals. Release addon build passes.

Logs: `debugging/p7-near-bound-control.log`, `p7-near-bound-chunks.log`, `p7-near-bound-action.log`, `p7-near-bound-compact.log`, `p7-near-bound-ts.log`, `p7-near-bound-typecheck.log`, `p7-near-bound-native-build.log`.

## Actual STM32 measurement and decision

Artifacts: `debugging/p7-near-bound-stm32-250s/`. The original RS485_B geometry/ports are retained; cold response cache, singleton covers, one Rayon thread, four allowed enrichment iterations, no result/cover memoization or section hints. Native/supervisor limits: 240/250 s, memory budgets 12/14 GiB.

The first cover records 9,345 bounded omitted blocks, 39,126,597 visited pairs, 3,458,266 far blocks, 15,362,486 planned near pairs and 736,044 exact-zero blocks. The preceding unpruned singleton cover had 39,743,535 visits and 15,604,494 near pairs: a reduction of 242,008 near pairs, about 1.55%. Cover wall time is 27.987756 s, versus the earlier 28.108834 s; this is not a controlled paired runtime benchmark.

The run ends at native deadline after 243.625 s outer wall, during its first action. Executed prefix: 308,531 visits, 48,053 far and 89,056 near callbacks. Kernel prefix: 655 adaptive calls / 20.419495 s and 88,401 separated calls / 84.082769 s. Sampled private/RSS peaks: 1,879,339,008 / 1,069,023,232 B. No completed magnetic action or electrical response.

The bound is correct on the focused controls but its measured work reduction on this STM32 case is small. Keep it disabled by default and do not extend this branch as the main acceleration path without stronger evidence. It may serve as a bounded-uncertainty mechanism for genuinely small volume corrections, but that use is not established here. The full ten-minute goal, hybrid electrical interfaces and the entire physical case-matrix remain open.
