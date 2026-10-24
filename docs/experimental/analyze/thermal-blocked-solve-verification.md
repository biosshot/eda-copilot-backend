# Bounded supernodal prototype: rejected

The approved small prototype does not improve the faithful10mm Cu/resin/via/
package/event coupon. Production remains on simplicial LLT; no selector, cache,
physics, tolerance, mesh, time controller, public API or resource limit changed.
No full TPS/Band/controller/curve requests ran. Whole P9 remains blocked.

The pinned faer0.24.4 already supplies supernodal symbolic/numeric LLT and
forward/back solves. The internal test-only candidate uses fundamental
supernodes (`relax: None`), the same AMD permutation and assembled SPD matrix,
sequential CPU/F64, and disabled regularization. It is not a new physical operator.
The old single-slot cache remains rejected. Three existing resident slots are
retained; the oldest slot is released before replacement factor construction.

## Predetermined comparison and result

Before any experiment, `blocked-solve/predeclared-gates.json` fixed at least10%
full coupon process-wall improvement, worst candidate replay <=90% of best
baseline replay (three repeats each), no planned/observed peak growth and maximum
complete temperature-field difference1e-7K. Original native coupon128MiB/30s,
geometry/materials/source5s/ambient20s and observation axis[0,5,20,60] were retained.
Each isolated process was bounded384MiB/120s; neither cap was approached.

| Metric | Simplicial baseline | Fundamental supernodal |
| --- | ---: | ---: |
| Complete coupon native wall |769.168ms|1151.802ms|
| Complete fresh process wall |877.380ms|1251.993ms|
| Complete coupon triangular timers |337.862ms|692.572ms|
| Complete coupon symbolic |1.016ms|1.174ms|
| Complete coupon numeric construction |15.047ms|16.368ms|
| Complete coupon packing |1.573ms|4.186ms|
| Complete coupon permutation |14.045ms|14.418ms|
| Total planned peak |36830775B|36971607B|
| Preconditioner planned peak |2016535B|2157367B|
| Three resident numeric factors |468384B|751944B|
| Observed peak working set |13967360B|14286848B|
| Maximum sampled private bytes |6721536B|11710464B|
| Accepted/rejected steps |902/9|902/9|
| CG iterations / factor builds |12514/23|12514/23|

Native coupon wall increased49.75%; fresh process wall increased42.70%. The
same executable was used in both fresh processes, with identical scalar
instrumentation; native and OS times are distinct measurements. Full coupons
ran exactly once each, so no full-run variance claim is made. Observed OS peaks
are distinct from the conservative allocation admission plan; private bytes were
sampled every10ms. None of these failing gates is promoted to acceptance.

The separate faithful matrix/RHS capture contains four original capacity/dt+
tangent matrices (dt=.001,.01,.1,1s) and two fixed RHS per matrix. Every variant
replays the exact same sequences64 times per RHS; construction, upper-to-lower
CSC packing and permutations are included. Baseline total replay walls were
20.840/18.840/18.868ms, candidate35.444/31.731/31.970ms. Worst candidate/best
baseline=1.8813; even the best candidate is slower than the worst baseline.
This result is larger than observed replay variation and fails the predeclared
gain gate. Kernel-only speed is not substituted for end-to-end speed.

## Numerical and ownership evidence

Both complete coupons pass unchanged native original-equation/energy/adaptive
time guards. Maximum original nodal residuals are4.616662e-12/4.616641e-12;
normalized energy balances1.445292e-13/1.448522e-13; maximum accepted local
errors.95048176323/.95048176321. Full output coverage and event/observation
axis match; missing samples0. Maximum complete temperature difference is
2.842171e-14K. Full temperature/flow/power/energy/coordinate arrays and all
differences are retained, rather than just probes. Results are not bitwise equal:
supernodal numeric reductions change floating addition order. Prior ordinary
fields would therefore need a comparable new pair before any future acceptance.

Standalone42 size/conditioning/variant combinations cover n=0,1,2,63,64,65,129,
diagonal scale ratios up to1e10 and three simultaneous RHS. Extracted factors
also agree with independent dense forward and backward triangular references;
original assembled-matrix backward residuals are checked separately. Shared
immutable factors produce identical results with independent concurrent scratch.
Sixteen named dependency fixtures exercise baseline plus dt, capacity, tangent,
free-order, sparsity, boundary and barrel changes for both variants. All PASS.
No additional cache is introduced: numeric calls factor the supplied complete
matrix, and a different free set/order/pattern receives its own immutable Plan.
Plans take no board-ID reuse key and never carry factors between meshes. The
final free-order fixture uses asymmetric capacities/shunts so a permutation
cannot hide behind a symmetric chain. Insufficient admission and nonpositive
pivots also reject for both variants without regularization (four negative
cases). These final fixture checks PASS; the measured A/B kernel prefix is
byte-for-byte unchanged. The exact original A/B executable and original fixture
report are retained separately from the final fixture executable/report.

Admission counts the fundamental symbolic row arrays/adjacency, padding in
actual numeric `len_val`, lower CSC transport, numeric/solve scratch and all
three resident slots. Symbolic construction has its own pre-allocation upper
bound, including temporary compressed rows and parent scratch; no numeric
factors exist during initial Plan construction. The final bound is the maximum
of symbolic construction and resident factor/packing/solve peaks, rather than
incorrectly adding nonoverlapping phases. Replacement and ownership fixtures
verify admission and release-before-replacement. OS evidence also fails the
no-growth gate, independently of this accounting.

The new simplicial baseline also matches the preserved pre-prototype full
baseline bitwise for fields/flows/energies/guards/counters/memory. This comparison
uses saved arrays and adds no run. The candidate's different arithmetic is
reported separately and cannot inherit that baseline equality.

All numerical tests and both coupon runs are complete. The final normal
default-GPU addon build PASS (jobs1, isolated target,6m25s); no production
selector is changed. The source/runtime manifest is
`debugging/thermal-verification/blocked-solve/evidence.json`. Kernel/end-to-end/
memory/full-field-difference graph `comparison.png` was viewed. Exact original
A/B and final fixture executables, requests, RHS/factor captures and raw logs
are retained. The earlier measured performance package and all its original
snapshots remain preserved. Heavy slot released; this task is stopped.

This candidate is rejected. No conservative whole-board<=600s preflight or new
ordinary/tighter pair is proposed from a slower, larger small prototype.
