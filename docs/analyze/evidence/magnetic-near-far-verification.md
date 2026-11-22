# P7 magnetic near/far verification

Bounded internal prototype completed; full P7 and ROI extraction remain open.
The fixed coupon passes the full 5% energy screen, while coarse theta0.7 fails
the separate 1% far budget. Theta0.4/0.2 pass that budget. ROI cost remains a
blocker. No full ROI solve, public DSL/PCB/vias, C/P6/Thermal or placement change.

## Geometry, admission and implementation

Task: next-task-magnetic-near-far.md, copied alone from MAIN. The accepted
finite-prism kernel, far.rs moments and prior frozen references are unchanged.
New local/near_far.rs provides a median spatial tree and unordered dual-tree
partition. Leaf balls cover complete support balls, not just centroids.
Same-node blocks recurse LL/LR/RR and self pairs always remain near.
Policies theta0.7/0.4/0.2 and leaf8 ROI/leaf1 coupon were fixed before runs.

A far block requires (rA+rB)/distance <=theta<1, with outward F64 guards.
The plan retains every support and all pairs. Foil XY and axial Z families are
planned separately because their current-vector dot products are exactly zero
under the existing model; this is not geometric or small-value pruning.
Blocks compress storage. Far application still streams individual degree-four
Moments::pair interactions: this is **not** an FMM or subquadratic action.

ExactCache refuses >64 supports before allocation. It caches directed accepted
near integrals, including separate forward/reverse values without averaging.
The full changed coupon has576 directed kernel evaluations exactly once;
all three policies and zero/weak controls reuse them. ROI receives planning
only, never this cache. Pair/node/block/typed-byte plans and deadlines guard
construction and action. Explicit limits:512MiB/60s,8192nodes,100000blocks,
6Mpairs. No cap increases. The conservative per-support allowances cover
input geometry, temporary current/action storage and source potentials.

Two translated copies of the accepted12-prism foil give24 supports and four
continuous P1 streamfunction modes. Geometry/current densities are frozen in
magnetic-near-far/coupon.json. Each island retains exact finite35um metal,
all exterior normal currents vanish, and internal normal flux is continuous
(up to inherited2.81e-17A/A rounding). No artificial return path or inverse(C).

## Full physical energy bound

For an unordered far pair let Rij be the existing analytic scalar remainder,
Vi its volume, and Ji(q) its full vector current density. Weighted Young gives

    2 Rij |Ji dot Jj| <= Rij (Vi/Vj |Ji|² + Vj/Vi |Jj|²).

Accumulate row_i += Rij Vi/Vj and row_j += Rij Vj/Vi. Then

    |q^T deltaL q| <= q^T B q
    B_ab = 1e-7 sum_i row_i (density_ia dot density_ib)
    |deltaU(q)| <= 0.5e-7 sum_i row_i |Ji(q)|².

B is a sum of positive Gram matrices. This bounds every physical coefficient
vector, including cancellation, rather than a collection of pairwise relative
errors. Tighter theta gives a subset of far pairs with unchanged per-pair
remainders, so rows and B decrease in Loewner order. Saved rows verify this.
B +/- sym(Lapprox-Ldense) are PSD in the saved full four-mode comparison;
the smallest tested slack is zero, with no clipping.

For field action the implementation separately accumulates
1e-7 sum_j Rij |Jj,d| per cell/component. Every tested arbitrary-vector
field difference lies below this absolute bound plus separately stated F64
roundoff guard. Responses agree with matrix action. All self/mutual entries,
signed error spectra and physical eigenvalues are saved, without clipping.

**Scope of rigor:** the analytic degree-four remainder and Young aggregation
are strict in exact arithmetic with exact moments/near integrals. F64 guards
are not interval arithmetic; near quadrature and empirical reference uncertainty
are not included in this strict analytic remainder. They remain separate.
The optional beta/(1-beta) relative certificate uses B normalized by Lapprox
when beta<1; it is conditional on those same exact-near/moment assumptions.
It does not certify total continuum accuracy.

Zero and nearly dependent physical representations are handled through cell
currents directly, never through an unstable q^T L q cancellation or tiny
energy denominator. Duplicate basis q=[1,-1] gives exactly zero energy/bound/
field. A1e-12 perturbed duplicate gives energy1.69748e-34J and absolute
bound7.83693e-36J. No relative certificate is offered for these representations.
The scale128eps*max(diag L)*||q||² classifies near-zero only, never drops currents.

## Independent reference and fixed gates

Self-island blocks reuse the frozen independently scrambled positive4D,
exact-double-z reference by rigid translation. The144 separated cross pairs
use positive triangle Gauss4/6/8 with an independent exact double-z formula;
no native integral or moment implementation is imported. All raw scalar and
physical matrices are retained. Frozen old reference hashes are unchanged.

The inherited physical self uncertainty is reweighted on the full four-mode
reference:0.4009630856%. Twice cross last-change is3.24e-15 relative.
A1e-12 numerical screening allowance gives total working uncertainty
0.4009630857%. This inherited statistical/observed estimate is explicitly
**not a certified continuum enclosure**. Full error uses the spectral norm
of reference-whitened sym(L-Lref); symmetry is used only for quadratic energy,
and the separate raw reciprocity gate remains1e-9.

| theta | near/far pairs | native full error | +reference uncertainty | aggregate far bound | 1% far gate |
|---|---:|---:|---:|---:|---|
| 0.7 | 138/162 | 0.04667700% | 0.44764009% | 4.85612420% | FAIL |
| 0.4 | 159/141 | 0.04650640% | 0.44746949% | 0.22227491% | PASS |
| 0.2 | 284/16 | 0.04650587% | 0.44746895% | 0.00217979% | PASS |

Dense full error is0.046507%. All four physical eigenvalues are positive
(approximately0.3152..0.3633nH); raw reciprocity5.55e-11 passes.
The accepted near-kernel0.2% primitive controls remain applicable; new dense
coupon comparison is an empirical full-operator check, not a proof that near
quadrature has a uniform absolute enclosure. Observed far-vs-dense full errors
decrease2.2473e-6 ->1.3974e-7 ->2.1612e-9 under fixed refinement, while
analytic bounds decrease0.04856 ->0.002223 ->0.00002180. The failed coarse
far budget is preserved, not accepted merely because observed error is small.

## Count-only ROI plans and cost blocker

All three saved ROI geometries are unchanged. The table combines foil/axial
plans. Typed plan bounds include reserved block capacity and conservative
scratch allowances. Near-cache upper storage is hypothetical: no ROI cache
was allocated. Construction across all18 group/policy plans took0.184s
including process/I/O; measured peak17.14MiB. Kernel evaluations:zero.

| ROI | theta | near | far | blocks | plan upper MiB | near cache upper MiB | near both directions + far seconds |
|---|---:|---:|---:|---:|---:|---:|---:|
| BandAmpV5 | 0.7 | 955,306 | 3,728,193 | 40,008 | 40.78 | 29.15 | 11359 |
| BandAmpV5 | 0.4 | 1,782,309 | 2,901,190 | 67,542 | 40.78 | 54.39 | 21210 |
| BandAmpV5 | 0.2 | 2,769,296 | 1,914,203 | 97,666 | 40.78 | 84.51 | 32967 |
| STM32G0_CAN_RS485 | 0.7 | 211,192 | 309,839 | 4,637 | 15.13 | 6.45 | 2509 |
| STM32G0_CAN_RS485 | 0.4 | 286,613 | 234,418 | 6,063 | 15.13 | 8.75 | 3407 |
| STM32G0_CAN_RS485 | 0.2 | 383,984 | 137,047 | 7,617 | 15.13 | 11.72 | 4567 |
| TPS63060DSCR | 0.7 | 654,250 | 1,884,857 | 26,509 | 32.11 | 19.97 | 7778 |
| TPS63060DSCR | 0.4 | 1,038,612 | 1,500,495 | 40,632 | 32.11 | 31.70 | 12356 |
| TPS63060DSCR | 0.2 | 1,553,315 | 985,792 | 57,406 | 32.11 | 47.40 | 18487 |

Costs are extrapolations, not ROI measurements or deadlines. They use actual
coupon near directed mean5.956ms and far moment mean0.257us (timing noise and
ROI shape distribution are not statistical confidence bounds). Near uses
both directions to retain raw reciprocity; far is symmetric and evaluated
once per unordered pair. The raw packet also saves optimistic single-near-
direction estimates and historical3.924ms refined packed estimates
(18377/2044/9963s Band/STM/TPS). Even optimistic single-direction near
costs exceed500s. No ROI physical energy-space bound has been established,
so coupon theta acceptance cannot be transferred to a real board.

## Validation and evidence

Focused Rust5/5 PASS (first4, then only the added independent test): complete unique coverage/self-near/refinement/separation,
movement/rotation/scale/mixed slender moments, full absolute bound and null
mode, and invalid theta/leaf/indices/densities/node/block/pair/memory/deadline/
>64-support operator admission. Initial compile26.88s, tests0.03s, jobs1 isolated. Added mixed-slender
6D direct quadrature6/8 test compile3.20s/test0.04s passes; no prior green
tests repeated. Maximum moment error2.50e-5 relative is enclosed by the
analytic remainder; reference last-change<=3.36e-13. Increasing separation decreases
the bound; rotation/scale pass. Bbox-separated but overlapping moment balls
correctly force all-near. Scalars and full logs are saved.
The source-only optimized harness compiles actual prism/polyhedron/far/
near_far code; no dependency or addon API change. Prior green magnetic
regressions were not repeated; accepted kernel source hashes are unchanged.

Bounded native coupon3.465s/4.30MiB; independent reference0.370s/34.43MiB.
All process caps512MiB/60s passed without retries. The heavy slot was
coordinated with P6/Thermal and released to Thermal after focused tests.
Root AGENTS normal default-GPU addon build also PASS, jobs1,17m39s from
fresh release cache. Binary/DLL hashes match; log retains20 existing packer
warnings and1 informational linker-stdout warning, no new-module warnings.
Public binding/API integration is outside this task.

Tracked fixtures/tools live under
tests/fixtures/analysis-local-extraction/magnetic-near-far/.
Detailed ignored packet:
debugging/p7-design-checkpoint/magnetic-near-far/
with audit.json, viewed audit.png, source/compiler/binary hashes,
full raw matrices/actions/bounds, build/run records, focused-test record and
manifest.json. Reproducible saved-value audit/plot do not launch solvers.

The next proposal is a separately reviewed block-aggregate far action and a
plan-only refinement of expensive near interactions, with physical ROI current-
space admission and its aggregate error certificate first. It must preserve
all supports, accepted near physics, absolute weak-mode treatment and caps.
No such stage, large ROI solve, reduction or public integration is started.
