# Lazy direct cubature preparation

The opt-in axial streamed path previously prepared order-4/6/8 volume rules
on every geometry-cache miss: 792 points per cell, even when axial evaluation
or the analytic near path did not use them. It now constructs geometry-only
cubatures and prepares just the selected direct rule after final axial refusal
or order-4 selection. Successful axial retries retain no direct points.

The original eager constructor still produces the same samples and arithmetic.
`prepare_direct` commits temporary arrays only on success, is idempotent and
reports whether it built a rule. Empty direct evaluation explicitly refuses.
No interior mutability or Sync change is introduced. The axial default remains
off; full worst-case cache/worker admission is retained because all three orders
can eventually be populated. No geometry, current DOFs or tolerances change.

Controls compare eager/lazy direct bits at 4/6/8, require the captured retry to
leave points empty, check order-4 and unsupported axial fallbacks prepare only
their selected order, and retain serial/parallel, cache and physical action
controls. Parallel progress now reports `lazyDirectRuleBuilds` and
`lazyDirectPointsBuilt`; these count lazy preparation only, not default eager
arrays. The counters include work in failed windows, as existing counters do.

A bounded preparation-only replay uses 32 original captured tetrahedra, four
alternating eager/lazy passes, 32 fresh caches per pass: 4096 cell constructions
per method. Eager preparation takes 0.0489225 s, lazy 0.0159514 s, ratio 3.067.
This small isolated cost is not a full-board speedup or evidence that preparation
alone removes the minutes-long bottleneck.

The external 120 s / 2048 MiB release supervisor passes 37 controls in 11.094 s in
`debugging/p7-lazy-direct-local-far-final-controls/`. Sampled peak private/RSS
is 25,935,872 / 38,506,496 B (20 ms lower bounds). The initial run also exposed
an old expected-refusal test requiring only KVL despite an earlier charge
refusal; it now accepts either exact physical refusal prefix, still requiring
refusal. No physical gate is weakened.

`npm run native:build` passes (`debugging/p7-lazy-direct-local-far-native-build.log`,
`BUILD_EXIT=0`). No additional full-board run is justified solely by this small
preparation-only result and the private undispatched model; the next bounded
board run should measure the dominant cover/adaptive costs explicitly.

The complete <=600 s extraction and all-case automatic shared-return route
remain open. No new board speedup is claimed from this control.
