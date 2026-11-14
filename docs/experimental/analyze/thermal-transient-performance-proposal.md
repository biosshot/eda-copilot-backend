# Measured transient bottleneck and next bounded design

This checkpoint does not accept an optimisation, a larger budget or temporal
convergence. No algorithmic candidate or new full-board acceptance run was used.
The prior single-slot cache remains rejected. Controller/curve runs stay deferred;
the earlier3600s/1536MiB estimate is not an accepted execution budget.

## What was measured

The preserved ordinary TPS and Band reports each perform seven real analyses:
stage0 graded seed, stages1–5 indicator-directed refinements, stage6 independent
nested global control. Each stage rebuilds a different FEM system and owns its
factor plan/cache; sharing factors across those meshes would be invalid. All
stages integrate the unchanged physical60s history. The saved stage times combine
geometry, assembly and integration and cannot be called factor/CG time.

One test-scoped native TPS tighter diagnostic uses the exact saved compact
network, layers/materials/contact/barrel/surface/initial state/source and ambient
events, geometry deviation.001mm and existing spatial policy. Native deadline110s
leaves bounded report-save headroom inside a120s process limit. It ends rejected
at stage1,t=9.551486345s; no partial field is published. Actual process110.207s,
native110.064s, peak RSS209932288B, maximum observed private209354752B. This fresh
Rust process is not a Node/public-worker memory or whole-request speed reference.

| Measured leaf phase | Time across seed and partial stage1 | Native wall fraction |
| --- | --- | --- |
| Simplicial forward/back triangular solve |51.241s|46.55%|
| Physical operator application |20.128s|18.29%|
| Residual/tangent assembly |16.056s|14.59%|
| Physical roundoff-bound evaluation |7.746s|7.04%|
| Mesh generation/control planning |3.477s|3.16%|
| Numeric factor construction |2.697s|2.45%|
| Apply gather/allocation/scatter workspace |1.348s|1.23%|
| Symbolic analysis |0.078s|0.071%|

Inclusive `linear`86.574s and `preconditionerApply`52.655s overlap these children;
they must not be added to the table. Compile0.188s and output0.306s are also
measured. Adapter/IPC serialisation is outside this native diagnostic. Profile
storage is fixed16 scalar stages,13 timers and8 counters, without matrix/field
copies; production compiles diagnostic calls to no-ops and exposes no new DSL.

Counters:6054 linear calls,26322 CG loop iterations (including the interrupted
iteration),2005 accepted/18 rejected time steps,40 factor builds and6014 cache
hits. Only4 selected factors have identical dt+tangent bits;0 exact-dt matches
were bypassed by the current selection. These are cache-hit classifications,
not a total count of all mathematically repeated matrices. For this constant
linear TPS problem the two half steps in every completed step-doubling attempt
have identical C/(h/2)+G: capacities/free indices/distributed terms stay fixed,
and source/ambient events alter the RHS. General nonlinear cooling can change G.
The exact-dt factor selection count must not be confused with that operator fact.
The retained request audit asserts constant cooling derivatives and no moving
distributed heat source; accepted+rejected counters prove at least2023 completed
identical half-step matrix pairs in this diagnostic (unfinished attempt excluded).
This is an explicitly source-derived lower bound, not generic nonlinear reuse.

The completed diagnostic seed uses19550 CG iterations/1483 accepted steps versus
the saved ordinary seed10268/734; factor builds only28 versus26. Disjoint timed
seed components already sum70.91s, versus42.93s total ordinary seed. This is not
a measured complete tighter runtime or a safe whole-request upper bound, but it
explains why a small cache/setup improvement cannot establish a600s preflight.

## Decision and concrete next prototype

No safe production gain is established by this measurement. Eliminating every
apply-workspace cost could save at most1.23% of this diagnostic, so workspace
reuse is not presented as the solution. Numeric+symbolic construction is only
2.52%; another factor-cache policy does not address the largest cost. Preferring
an already cached exact-dt factor has zero observed opportunities. The rejected
single-slot result increased CG91.2% and wall55.2%, directly harming the dominant
triangular work. No such policy is restored.

The next useful **small prototype**, requiring review before further work, is a
blocked/supernodal forward/back solve for the same admitted SPD preconditioner,
benchmarked against the retained simplicial solve on the predetermined faithful
10mm Cu/resin/via/package/event coupon. Preserve the physical operator and all
independent residual, energy, time and spatial gates. Capture and replay identical
factor/RHS sequences before comparing full coupon fields. Separate symbolic
packing, factor storage, solve scratch, permutation and kernel wall time. A2x
triangular speedup would save only23.3% of the measured diagnostic wall, not2x
the full simulation; it is not evidence that either complete tighter request
fits600s. No whole-board preflight below600s is currently established.

Any packed factor must remain owned by its System/immutable Plan; mutable solve
scratch belongs to one linear call and cannot be shared between concurrent
contexts. Admit padding/workspace and every resident factor before allocation,
including transient replacement peak. No extra unaccounted factor slots. A
candidate must not worsen planned or observed memory peak on the same case;
merely fitting under384MiB would not satisfy this task's comparison condition. A
factor key must include dimensions/free-index order, complete sparsity,
capacity/dt diagonal and all ordinary/distributed/boundary tangent coefficients
by exact bits; source hash or dt alone is insufficient. Changed mesh, capacity,
cooling tangent, boundary/free set or barrel weights must invalidate it. No
factor is reused across different refinement stages merely because board IDs
match. Tests must explicitly mutate each dependency and verify ownership/peak.

Supernodal/blocked reductions may change floating addition order. Either prove
bitwise equivalent output for the chosen implementation or quantify full-field
differences and obtain a new comparable ordinary/tighter pair before acceptance.
The existing ordinary fields cannot silently serve as a baseline for a changed
numerical algorithm. The current task authorises neither extra ordinary real
runs nor a relaxed budget to bypass that requirement. Thus this checkpoint stops
with a specific measured design proposal; full real performance runs used0/2.

The only off/on A/B performed here tests instrumentation, not a speed candidate:
the same saved faithful coupon gives bitwise identical full temperatures, flows,
energy, quality, counters and memory plan.0.7645/0.7571s is timing noise, not a
claimed optimisation. The earlier input-only rejection of DSL `validity` metadata
at native Material deserialisation is retained; normalisation removes metadata
only and occurs before the single physical diagnostic. Error status/fields and
shared deadlines are not weakened. See current verification and the ignored
`performance-*` source/evidence manifest for commands, hashes and local charts.
