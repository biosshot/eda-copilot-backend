# Node-only adaptive classification terms

The common magnetic batch constructs its entire cover before calling any pair
kernel (`Plan::new_with_policy`, then `Plan::apply`). Its envelope is the maximum
of normalized current amplitudes over all columns. Chunk size one can exploit
each excitation's sparsity separately; it is not equivalent to merely scheduling
the same cover earlier. Default common batching is retained.

`Plan::decide` previously recalculated two node-only interval expressions on
every visited pair: the quarter-mass action allowance and the permeability-scaled
maximum source weight. They are now computed once after the mass aggregation,
using exactly the original interval operation order and directed rounding.
Per-node deadline checks remain. Classification uses lookups; no geometry,
tail order, error threshold or zero/near/far policy changes.

Admission reserves another `sizeof(f64)+sizeof(Interval)` = 24 bytes per tree
node, in both common and chunked paths. This trades bounded linear storage for
less arithmetic across millions of visits. A board speedup is not yet measured.

Controls compare original uncached decision formulas with the precomputed
decisions over all ordered pairs in an overlapping/distant-cell fixture, both
near-bound policies and two widely separated goals. Far and unresolved decisions
are required to appear. Additional checks compare node terms bitwise and retain
duplicate/self omission controls. Existing signed actions, interval bounds,
cover metadata and serial/2/4-worker comparisons remain unchanged.

The release controls in `debugging/p7-node-budget-regression-controls/` pass
31 tests under an external 120 s / 2048 MiB ceiling, wall 13.109 s. Sampled peak
private/RSS is 20,205,568 / 35,172,352 B (20 ms lower bounds). Three additional
classification controls pass in the focused run. An initial runner incorrectly
included an isolated ignored test without its required environment; that failure
is preserved separately and the runner was corrected, without changing the test.

`npm run native:build` passes with `BUILD_EXIT=0` in
`debugging/p7-node-budget-native-build.log`.

All case-matrix families and production fallback paths are preserved. This
optimization establishes no new physical case acceptance or completed channel.
