# Review: spectra, magnetic near/far and rejected Thermal prototype

2026-10-24. New assignments are deliberately not issued: user requested discussion first.

## Accepted bounded primitives

P6 signal kernel: reviewed transform/window/normalization/convolution and memory/deadline code plus pinned dependency/lock changes. Parent reran existing built focused tests: 5 PASS, benchmark intentionally ignored. Independently recomputed saved convolution prefix and PSD integral: PASS. All33 evidence hashes match. Agent reports22 lib tests and one bounded benchmark; no public DSL wiring or full P6 closure.

P7 near/far: reviewed complete pair partition, exact-near cache, absolute Young/Gram aggregation and operator application. Parent independently verified PSD and B±sym(Lapprox−Ldense) on all saved policies; all51 evidence hashes match. Agent focused5/5 and own normal addon build pass. Coarse theta0.7 fails separate far budget;0.4/0.2 pass on the coupon. Whole ROI cost still fails, far action remains pair-streamed rather than subquadratic.

Source and fixture changes are integrated into main. Tests/build evidence belongs to the respective worktrees; combined main native addon was not rebuilt, and the existing main addon was not replaced with an older isolated-worktree addon. Public entrypoints are unchanged. Main's preexisting untracked native/analyze/debugging is preserved.

## Thermal: reject candidate, preserve experiment

All68 evidence hashes match. Supernodal candidate worsens full process wall by42.70% and raises memory. Numerical agreement does not override failed performance gates. Production selector remains simplicial; no full-board runs. The verification report is retained here; prototype/source/output evidence stays in analyze-thermal. This is neither acceptance of the candidate nor closure of P9.

## Discussion, not assigned work

P6 now has useful internal electrical/signal primitives; extending unrelated helpers is lower priority than a complete board path. P7's largest barrier is still costly near interactions and the size/quality of the physical discretization; speeding far action alone cannot resolve the measured near cost. Review geometry/basis/near kernels before choosing another acceleration implementation, without silently deleting supports or weakening accuracy.

Thermal needs a decision between a separately bounded longer verification run (to separate temporal accuracy from the600s execution goal) and further measured algorithm work. The rejected supernodal candidate supplies no justification for an automatic rerun or larger controller run. Any change of acceptance resource target must be explicit; the physics gates remain unchanged. Full P8/P11 board functionality still depends on a supported, verified extraction path.
