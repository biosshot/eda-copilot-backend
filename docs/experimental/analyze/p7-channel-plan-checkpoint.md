# P7 closure implementation checkpoint — 2026-10-29

This is a partial implementation of A from `p7-real-channel-closure-plan.md`.
P7 closure is **not achieved**. Existing accepted P6/P7/cache work is retained.

| Obligation | Implemented/verified here | Remaining |
|---|---|---|
| A: no mandatory authored scope | `channel(from,to)` reaches automatic native-geometry/island planning; direct DSL TOP/BOTTOM finite-via coupon passes adapter test | Full numerical no-scope real-board acceptance |
| A: physical branches/returns/environment | complete confirmed islands; all branches retained; surrounding intersecting copper added to domain closure; no net-name joins or shortest-path substitution; automatic requests forbid insulated cuts | homogeneous path segmentation, small relevant environment with expansion convergence; internal contact faces; authored path constraints; arbitrary optional scope shape |
| A: inspectability/cache | plan includes region IDs, contacts, ROI, island, closure iterations, geometry diagnostics and explicit unverified spatial status; attached to channel segments/solution; immutable native board snapshot reused by WeakMap lifetime | process-restart preparation cache; tested broader-band model reuse |
| B | existing templates and fast path preserved | polygon meshing/material regions, arbitrary conductor/mode matrices, board slicer and independent accuracy matrix |
| C | existing finite direct-FEM and P6 integration preserved | section/local composition, arbitrary discontinuities, internal contacts, return-slot acceptance |
| D | [HF operator prerequisite](p7-hf-operator-design.md) identifies the missing conforming surface-current operator and transition/causality checks | DC extension, diffusion overlap, HF local physics, dielectric dispersion, request-dependent certified band, positive real RF/time channels |
| E | existing factor ownership/pool/cache unmodified | 1/2/4-job end-to-end measurements; segmented model reuse and 300s/2GiB cold acceptance |

Conservative complete-island domain closure can include a large return plane and
therefore most of a board. This is not the requested final path-local extractor.
It avoids cutting a real return plane and lets the native budget reject an
oversized problem. Bounding envelopes are intentionally conservative, not a
physical distance/convergence criterion. The planner caps 100000 regions and
256 closure iterations. Existing native geometry and FEM caps remain in force.
The automatic preparation occurs before the native cold deadline starts; wall
reporting now includes it, but one shared end-to-end deadline remains work.

Verification: `npm run typecheck`, `npm run build`; combined
`node --import tsx --test tests/analyze-local-adapter.test.ts tests/analyze-local-cache.test.ts`
passes 8 tests. Original STM32 explicit request is deep-equal to its unchanged
reference. Added positive no-scope test uses a finite coupon derived by retaining
the two real captured vias, not the complete real board. Added branch/floating
neighbour/disconnected-endpoint tests verify planning only. Cache tests cover
cold/memory/disk restore and corruption/invalidation mechanisms, not new numerical
model timings. No Rust operator was changed; no numerical native rebuild or
expensive full sweep was necessary for this checkpoint.

One complete STM32 board planning probe was attempted, without a native field
solve. It refused unresolved `drill_topology_uncertain` at tracks 415/418/460 and
`contact_uncertain` at pads in the conservative selected domain. Saved evidence:
`debugging/p7-channel-plan/real-stm32-plan.json`; wall 0.97318s, process high-water
RSS 113238016 bytes. These are **planning refusal** measurements, not cold model
acceptance. No warm/restart full solve, five-percent observable accuracy, new
current/loss maps or reviewed visual result report is claimed. Original
`native/analyze/debugging/` and the user-authored closure plan remain untouched.
