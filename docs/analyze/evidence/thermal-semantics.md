# Thermal semantics and scope

This is an independent CPU/F64 thermal analysis through the existing DSL and
shared process/addon. T1 compact networks, T2 layered PCB and T3 prescribed
cooling remain distinct acceptance stages. Verification, rather than this method
description, determines completion. No CFD or electrical-temperature feedback.

Current acceptance checkpoint (2026-10-24): the bounded production-policy native
controller steady gate passes, including its independent nested control and
original nodal/energy guards. The fresh public direct DSL/shared-addon steady
gate also passes; complete three-board, curve and transient acceptance remain
pending; see the current result first in
[thermal-verification.md](thermal-verification.md). The earlier positive
test-only meshes are historical evidence, not substitutes for public execution.

## T1 equations, properties and time

Native refinement comparison retains separate absolute and normalized maximum
witnesses (time, layer, position, coarse/fine degC, ambient, rise denominator K,
difference K and gate allowance). Zero-rise witnesses have no relative quotient.
Missing boundary samples may use only unique cyclic CAD outer-ring correspondence
with identical edge fraction, reported Boolean displacement bound and unchanged
strict P1 lookup; holes and interior samples do not use this correspondence.
Both fields must carry the same nonempty source board identity. Bounded scalar
provenance is included in comparison memory admission; previous full fields are
not published. The single-slot preconditioner experiment is compiled only in
explicit Rust tests and changes no production cache or acceptance tolerance.

Existing `thermalNetwork`, `a.model`, `environment`, `initialC`, `initial`, `time`
and `domain` vocabulary is retained. Nodes supply capacity in J/K; environment
nodes have prescribed ambient temperature and no capacity/source. Links supply
positive K/W or cooling tables. Optional link `id` names heat-flow observations;
otherwise `from->to` must be unique. All properties originate in authored models,
never a package/material lookup. A temperature is junction only when the author
explicitly models a junction node.

`C dT/dt + incidence Q(T_from-T_to,t) = prescribed power`. Temperatures are degC,
differences K, flow W, energy J. Steady requires every physical connected component
to reach an environment boundary, including zero-power components: a floating
unloaded steady component is underdetermined. Transient requires explicit positive
capacity and initial conditions for each physical node. An isolated transient node
is valid because its initial condition fixes the temperature reference.

Cooling tables contain finite strictly increasing delta-T and heat flow, include
the exact point (0 K, 0 W), and have positive slopes. Interpolation is linear;
extrapolation and implicit odd reflection are forbidden. Author negative points
to permit reverse heat transfer. Optional `scale: Waveform` is dimensionless,
nonnegative over the full waveform and multiplies the whole cooling law. Zero
disconnects that link. No false steady reference is supplied by a disabled path.

Matrix-free conjugate gradient solves the symmetric
positive definite tangent system. Internal residual is stricter than the required
1e-8; residuals are verified by matrix application using physical differences.
Systems below 64 free unknowns use Jacobi. Larger steady and transient
systems use sparse Cholesky of the ordinary tangent plus full finite-barrel outer products as an SPD preconditioner. The physical matrix-free operator, source and
mass remain unchanged, including every distributed barrel path. No diagonal
regularization is enabled. AMD ordering and elimination-tree column counts are
computed with O(n+m) storage before factor fill is allocated. Barrel contact
pair counts are checked before expansion against a conservative pattern-memory
estimate within the admitted preconditioner allowance. The earlier internal 250000-entry
heuristic was rejected; physical node/link caps remain unchanged. Unadmitted products fall back without allocating them. The admitted plan
includes indices, one steady or three transient numeric factors and workspace within the configured request memory budget. The physical-system plan remains
`640*n + 384*(ordinaryLinks+paths) + 64*contactTerms + 160*sources` bytes; a
quarter of the request is retained separately for transport/report headroom. M
may use only the remainder. Before allocation it counts input contact pairs,
unique nonzeros and symbolic fill, accounting for simultaneously live pattern /
analysis data, input contributions, symbolic indices, admitted numeric factors,
workspace and solve buffers. Published memory-plan partitions sum to at most the
unchanged request budget. These are conservative allocation estimates, not a
strict cap on OS RSS. Unadmitted or nonpositive factors fall
back to Jacobi; a deadline is still an explicit failure.

At most one steady or three transient factors are retained, evicting before a
new allocation. After symbolic fill is known, the largest cache capacity that
fits this remaining allowance is admitted (one, two or three); the actual cache count is
reported. Actual pair counts, unique nonzeros, symbolic fill and estimated
bytes are published, including in deadline diagnostics. A factor
can be reused for an unchanged tangent and a timestep within a factor of two of
its own timestep; steady factors require an unchanged tangent. This changes only M, while every CG solve still verifies the
original A and residual. This prevents refactorization at every adaptive step.
Counters report CG iterations, factor builds/fallbacks and planned factor bytes.

Every solved Newton/BE state independently verifies the **original** nodal
equation, not only its CG correction: `||F(T)||2/(1 W+||b_ambient||2) <= 1e-8`. The explicit 1 W normalization floor makes the ratio dimensionless.
Here `b_ambient` is the original residual evaluated at uniform instantaneous
ambient: source powers in steady; source plus `C/h*(old-ambient(t))` in BE.
The Newton rounding allowance is capped by this physical bound. Global power
balance cannot substitute for local equilibrium. A stiff/weak parallel-branch
regression has zero global balance at an initial state that is wrong by 100 C;
it must solve correctly or reject, never complete using a max-diagonal floor.
`quality.originalNodalResidual` reports the maximum normalized accepted-step value.

Internal state uses F64 coordinates `T-temperatureOriginC`. Time uses the fixed
initial ambient origin (or the finite verified origin restored from continuation);
that origin stays fixed over the complete time trajectory. Steady uses the explicit common heat-balance estimate
`ambient+sum(Q)/sum(G_ambient)` only for exactly linear environmental paths.
A nonlinear environmental table or zero total G uses the declared ambient origin;
there is no guessed steady heat-balance conductance. This is only a coordinate choice, not a new boundary/model. Internal
flows, storage and original residual operate on these coordinates; absolute C is
reconstructed only when publishing temperatures. Continuation also retains full
`temperatureCoordinatesK` and the origin, checking finite values, length and exact
agreement with published temperatures before reuse. It preserves gradients that
would be lost by repeatedly adding them to a large absolute temperature. Public
absolute Celsius arrays retain normal F64 display rounding; small physical drops
can therefore differ from subtraction of rounded public temperatures. Saved
coordinates, common origin and authoritative heat flows retain the solved precision.
Fixed-fill IC(0) was tested and rejected for the real board stencil because all
attempts fell back and the transient hit its CPU deadline; it is not another
production engine.

Steady linear corrections use the exact affine representation `deltaT = c + y`,
where `c = sum(rhs)/sum_i u_i` and `u_i` is the sum of tangent conductances from
free node i to the fixed environment. Internal FEM, compact and barrel paths
annihilate a constant exactly through local differences; `u` is assembled from
boundary paths, never from cancellation in a global A*1 multiplication. CG solves
`A*y = rhs-c*u`; final residual verification evaluates `A*y+c*u-rhs` in the
original physical norm. Outputs add the common correction only after verification.
The analytic cosine-ring test compares shifted/unshifted representations and
ambient offsets 0/25/125 degC. The saved controller assembly audit compares full
sparse M with independent physical A on four vectors, rejects an injected second
barrel diagonal and checks common-shift invariance of every physical residual.

The requested CG residual is `1e-12 + 1e-11 * ||rhs||2`. A separate explicit
F64 backward-error allowance prevents chasing an unrepresentable residual on
sliver/high-contrast meshes. For the current CG vector `x`, let
`s_i = C_i/h*|x_i| + sum_links |g_ij|*(|x_i|+|x_j|)
+ sum_paths G_e*|w_ei|*sum_j |w_ej*x_j|` (fixed corrections are zero;
C/h is absent for steady). Let `p_i = 2 + 4*linkDegree_i
+ sum_incidentPaths (termCount_e+4)` and `b = ||eps64*p_i*s_i||2`; the affine solve adds the explicit
`4*eps64*||c*u||2` allowance and applies this formula to its relative vector y.
The independently recomputed actual residual can stop at
`max(requested,b)` capped by `1e-8*(1+||rhs||2)`. Final verification also caps
its original eight-times-requested margin at the same unchanged physical limit.
Reports retain actual normalized `linearResidual`, the normalized computed
`linearRoundoffBound`, and the effective `linearStoppingAllowance`. A negative
unit test proves no floor, including infinite/NaN candidate floors, can admit
an actual residual above the physical 1e-8 guard. Temperature, temporal,
convergence and energy guards are unchanged.

Newton with damping solves tabulated links without leaving their declared domain.
A documented floating-point residual floor handles large C/dt. Failure to solve
is distinct from a successful hot model that fails a temperature check.

Backward Euler with two half steps and step-doubling error control uses adaptive
internal steps. Defaults: relative local tolerance 1e-6 of temperature rise/local
change, absolute 1e-7 degC, attempted step limit 200000. Report times do not define
the integration step. Power/ambient/cooling events terminate internal steps;
discontinuous endpoint values use the left limit in integration and right limit
in observations. Sines additionally cap steps at 1/32 period to prevent aliasing.
Accepted half steps integrate input and ambient energy with the same BE quadrature
as the state update. Stored energy uses per-node initial temperatures, avoiding
cancellation from an arbitrary absolute energy reference. Sampled extrema can
miss intersample peaks.

`initialC('ambient')` means ambient at the simulation start; explicit initialC is
uniform. `initial('steady')` solves the initial source/ambient/cooling state.
Continuation contains a versioned SHA-256 identity, node temperatures and final
absolute time. The hash streams the complete canonical representation of context,
network coefficients/topology/source and ambient profiles, distributed barrel/contact
paths and the spatial geometry/material/provenance report. Object keys in the spatial
JSON are sorted recursively; arrays preserve order. No serialized network clone is
retained. Changing these properties rejects continuation, including boundary profiles,
barrel coefficients and board identity. Output times, numerical/resource budgets and
initial guesses may change; incompatible identity or node count rejects. A resumed stop time
is absolute and must exceed that state time. Steady may use initialC as a Newton
guess but has no time integration or continuation.

## T2 geometry, coefficients and compact coupling

Parent-approved additions are thermal `mesh`/`inputPolicy`, optional link id,
local thermal model `contacts: [{node,target,resistanceKPerW}]`, and cooling scale.
Copper analysis stackup entries require explicit `backgroundMaterial` for resin
between copper shapes. It stays in authored input; RoutingStackup transport and
RoutingBoardV2 are unchanged. Each material requires k and source/state; transient
also requires density and specific heat. No implicit air, FR4 or copper properties.

`mesh.geometryDeviationMm` uses the DC geometry tolerance: default .001 mm,
accepted range .000001.. .01 mm. A different value must be explicitly authored;
node-budget failure never coarsens curves automatically or removes source objects
or holes. The report records the requested tolerance, prepared per-region curve
approximation bounds, their maximum and the Boolean error bound. These are error
bounds of the actual prepared geometry, not an exact measured geometric error.
Ambiguous topology or contact coverage remains a rejection. Captured-controller
acceptance independently compares .01/.005 mm curves at fixed mesh and progressive
meshes at .005 mm curves. The starting 4/2 mm pair is retained as a failed
resolution study when it exceeds 2%; subsequent 1 mm / .5 mm levels remain subject
to explicit node/memory/link budgets. The accepted last pair requires <=2%
junction-rise and full-field change, with location/error audit and conservation.

The common conforming P1 mesh follows the actual board outline, cutouts and
projected prepared copper contours from every layer, not
the electrical copper-only mesh. Nominal `maxEdgeMm` spacing is refined locally
to one eighth that spacing around selected pad/component source/contact footprints.
Each authored stack slice retains its own temperature field at that slice's
through-thickness center. Outer source/contact and cooling paths act on these
center fields; there is no separate resolved outer-face temperature. Authored
finite contact/cooling characteristics must represent any unresolved surface path.
Independent references use the same center positions and source averaging.
For clipping, indexed robust boundary predicates first prove whether any material
boundary enters the open triangle. Only after proving no cut does one interior
point identify its constant material class; boundary touches have zero area.
Cuts or ambiguous classifications use the original Boolean intersection. This
is a topological fast path, not centroid-based cut-cell material assignment.
Physical prepared
copper unions intersect every triangle: stiffness integrates copper/background
area against constant P1 basis gradients. Copper material interfaces are constrained
in the shared planar mesh. Remaining cut cells (notably compact barrel footprints)
use exact overlap coefficients and retain an approximation needing refinement.
Projected contour segments from all copper layers and the outline are explicitly
planarized using robust pair intersections and shared intersection vertices
before noncrossing CDT constraint insertion. Original coefficient polygons remain
unchanged; the report records the maximum measured floating-point displacement
of split points from source segments as `mesh.constraintDeviationMm`. Unresolved
constraint crossings or split displacement above 1e-9 mm reject explicitly; no contour is dropped or snapped. Pair,
split-point and vertex budgets are bounded before triangulation.
Optional interior sampling points within 1e-9 mm of an existing contour segment
are omitted to avoid duplicate interface slivers; physical contours do not move.
Selected source/contact pads receive a local lattice only on their exact union
plus a one-spacing geometric halo. Spacing is min(maxEdgeMm/8, minimum authored
primitive pad width/2); the report retains the width, derived spacing, halo,
candidate and inserted counts. Arbitrary polygon pads do not acquire a width
inferred from area or bounding boxes; their width is reported unsupported and
spacing remains maxEdgeMm/8, requiring explicit mesh/field convergence. Primitive
nominal width does not resolve arbitrary necks or annular webs automatically.
Spatial maxNodes defaults to 50000, accepts explicit values up to 100000, and
checks both unpruned replicated mesh and the complete physical network. The
controller's initial resource study explicitly used 60000, then 100000 for 1 mm.
The original unpruned fine mesh had 53226 layer vertices before 47796 physical
nodes; the old 50000 rejection preceded support pruning. A separate, explicitly
authorized 0.5 mm measurement probe permits 110000 nodes, 500000 spatial links,
four million output numbers and 768 MiB. These probe ceilings are native admission
bounds; public DSL/default limits remain unchanged pending measured convergence
and resources. Its complete pre-pruning plan has 106476 replicated nodes, 35492
planar vertices and 8064 new contact samples. Rejected levels remain evidence.
Mesh/alignment convergence and geometry curve/feature-resolution limits remain
mandatory; stable global averages alone are insufficient.

`spatial.geometry.physicalDomain` is the authoritative polygon set used for all
layer coefficients: board outline minus explicit bores and the compact barrel
union. Basis vertices can lie in void while retaining support in neighboring
physical material. Physical-field audit classifies those vertices by this domain
and samples both meshes plus all boundary vertices/midpoints and inward traces;
missing physical support rejects rather than becoming zero. The sampler takes
an explicit ambient reference for temperature-rise normalization. It is not a
certified continuous maximum over the intersection of both triangulations.
The extra controller domain representation is 459077 serialized bytes. Barrel
reports retain center, outer radius and the 128-vertex rim convention for audit.

A private `cfg(test)` study separates fixed source-halo/global spacing and sizes
constraint edges as well as the interior without moving coefficient polygons.
Separate saved-mesh directional midpoint experiments retain all previous triangle
edges, re-integrate coefficients and conservatively rebuild contact/rim weights.
These paths expose no DSL/environment setting and are not production adaptive
meshing. Their heuristic reaction indicator does not certify error; the actual
full physical field and strict original residual remain acceptance requirements.


Barrel heat-capacity accumulation retains a bounded mesh-index to physical-node
index map. It visits the same normalized rim weights in the same order, avoiding
repeated whole-network string scans without changing coefficients or arithmetic.

In-plane stiffness is thickness times the integral of k times basis-gradient dot
products. Capacity is positive lumped mass, separately per layer. Through-thickness
transfer integrates `1/(d1/(2 k1)+d2/(2 k2))` on actual physical overlaps, with
separate partitions for all four copper/background combinations and vertex
lumping. For supported through bores physical support is shared across slices;
its triangle clips are computed once and reused, avoiding redundant full-board
self-booleans. Selected source/contact support is clipped once and triangle
candidates are selected by RTree. This preserves total power and uses harmonic half-layer resistances,
never arithmetic k across dissimilar layers. Dimensions explicitly convert mm
to metres and mm2 to square metres. Source/contact distribution integrates P1
basis functions over clipped selected polygons and normalizes weights to one.

The first spatial class is alternating copper/dielectric slices, supported
prepared copper, round through bores and board cutouts. Blind/buried spans,
plated slots, solder-mask thermal layers and unconfirmed independent plated drills
reject explicitly. Three direct saved-board scenarios use STM32G0_CAN_RS485 U3,
TPS63060DSCR U1 and BandAmpV5 U1 with a conditional radiator. A nonexistent MOSFET
is not invented. Geometry is captured CAD data; thermal parameters are explicit
conditional hypotheses, not measured component performance.

Entire barrel footprints are removed from layer coefficients/capacity. Finite
axial conductance is k times analytic annulus area divided by layer-center
distance, with normalized P1 interpolation of the outer-rim temperature at 128
angular points. The finite matrix-free path is `G*(weighted T_top-weighted T_bottom)`;
its assembly/application is linear in contact-node count, without dense pairwise
expansion or reference to unsupported nodes inside the bore. Each annular contact
must belong to one physical mesh component. Barrel mass is added once at those
contacts. This axial/annular approximation requires convergence;
it is not a full 3D barrel field. No ideal short joins all layers. Bore contours
and the geometry approximation bound originate in the shared preparation.

Compact components have explicit named nodes/links and finite board contacts.
Package name supplies no RthetaJC/RthetaJA/capacity. No case/board/ambient path is
inferred or counted twice. A radiator requires explicit heat capacity, TIM
resistance and cooling characteristic. Board-surface resistance/table means the
whole top-plus-bottom exposed-surface characteristic, partitioned by area, not
K*m2/W. Sidewall convection is absent. Active/passive cooling differs only by its
authored characteristic and scale, without airflow/fan simulation.

## Loss-map authority and power conservation

DC `lossMap.data.solutionId` resolves only against completed authoritative DC
solutions in this context and board. Validated meshReference is followed; every
supplied watt sample must equal its saved source. Foreign, missing, changed,
reordered, trimmed or uncomputed domains reject. Conditional status propagates.
Published DC reports do not duplicate meshes. Internal thermal requests receive
source triangles and finite barrel-segment power at dispatch. Segment power is
split equally between its two endpoint annular contacts, normalized there, so
all triangle and barrel watts are conserved. Missing barrel/contact identity
rejects. This is one-way prescribed-loss transfer, not P10 feedback.

## Evidence, reports and limits

Independent T1 references: one-node exponential, two-node modes, ambient step,
finite pulse, table steady solution and temporal refinement. Energy normalization
uses a stated 1e-3 W/J scale floor, equivalent to a separate 1e-9 W/J absolute
allowance at zero power/energy plus the original 1e-6 relative guard. T2 references:
slab L/(kA), dissimilar harmonic series layers and a reproducible continuum
Fourier thin-sheet spreading solution observed over the source area. An independent
layered cell-centered FV reference (test-only) additionally resolves a 400/0.3
W/(m K) contrast, 0.08 mm copper strip and two mesh alignments; its harmonic face
resistances, layer-center positions, Robin boundary and source averaging match the
FEM problem. Reference refinement stays below 0.5%; required FEM reference error
and final mesh change both stay below 2%. Near-zero
absolute tolerances are separate from relative temperature-rise errors. Required
guards remain <=0.1% analytic time error, <=2% spatial change, linear residual
<=1e-8 and normalized steady energy imbalance <=1e-6.

Reports retain complete node/link manifests, temperature/flow arrays, power and
energy ledgers, layer meshes/coefficients, contact/source weights, finite barrels,
properties and board/source identity. Thermal node/link `DataDescription` objects
are constructed lazily from these authoritative arrays when requested; public
temperature/heat-flow access, units and conditional diagnostics are preserved.
Missing, truncated or nonfinite observations remain unavailable. A temperature
map references its own solution ID and `spatial.mesh.fieldNodes`; the full mesh
is published once in that solution instead of copied into map metadata.
Limits: 50000 compact-network nodes or
100000 ordinary spatial nodes (authored mesh default 50000), 250000 ordinary/distributed
links. Explicit opt-in refinement permits up to140000 replicated spatial vertices,
600000 links and four million observation/state-coordinate numbers at the larger
mesh bound; ordinary limits remain unchanged. Both paths retain two million
contact weights, 100000 samples and their monotonic wall-clock,
planned memory and iteration budgets. Cancellation kills the existing disposable
scenario process. No native background thread or GPU is created.

The optional refinement currently supports copper/core/copper slices and explicit
component or polygon-region source/contact footprints. Other layered geometry
can still use the ordinary solver and separately authored mesh convergence.
Its seed uses background healing length ell=sqrt(k_sheet*t_sheet /
g_harmonic_to_core), fixed4ell source halo and graded ell/2,ell,2ell sampling;
constraints are subdivided in that halo as well. This is a reproducible heuristic
seed, not a universal resolution guarantee. All physical coefficients and source /
contact/barrel mappings are recomputed on each nested mesh.

Refined final reports additionally retain `coordinateFrame` with originC and
sampled coordinate rows in `system.nodeIds` order. Temperatures in Celsius are
rounded projections; field comparisons use the common-origin rows directly.
Only the final physical analysis has full public arrays. Intermediate scalar
history records actual counts, residuals, cost, simultaneous allocation plans and
sampled changes. Exhausted mesh accuracy leaves computed data conditional with
`THERMAL_MESH_UNCONVERGED`; physical/numerical or resource refusal is unavailable.

Versioned audits check complete arrays, axes, units, finiteness, identities,
sources/contacts, conditional status and balances. Named-board temporal refinement
compares every node at every saved time. Spatial audits interpolate both physical
nodal sets, authoritative boundary vertices/midpoints and inward void traces on
every layer, normalized by the fine field maximum rise above the explicit
reference ambient. Requested historical missed regions are sampled separately.
This sampled maximum is not a certified continuous L-infinity bound. Missing
physical support rejects the audit rather than becoming zero. Compact junction
rise is checked separately. Audits include deletion/trimming/duplicate,
unit/ID, missing-to-zero and conditional-to-valid negative substitutions. Each
claimed run saves exact input hashes, implementation/native hashes, JSON, audit,
HTML/SVG/PNG, wall time and process high-water RSS. Static visual QA from saved
arrays is not a browser screenshot and does not verify interactive controls.

The independently admitted 140000-node / 600000-link / 1024 MiB controller probes
precede the approved optional `mesh.refinement` implementation. The approved
policy and exact field-error normalization, event coverage, total budget and
unconverged semantics are in [thermal-mesh-production-proposal.md](thermal-mesh-production-proposal.md).
It is under implementation; previous positive experiments alone do not imply
production acceptance. A nested spatial comparison establishes observed
convergence for that geometry/model/sample coverage, not absolute thermal
accuracy or adequacy for a different transient, source, material or cooling law.
