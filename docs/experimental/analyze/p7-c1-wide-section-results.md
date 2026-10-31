# P7-C1 wide-section coefficients, 2026-10-31

Status: bounded internal coefficients-only implementation and positive independent coupled EM control pass. Real STM32 section admission remains blocked by B2 geometry certification; C/D/P7 are not complete. No native change, new solver, dependency, public DSL or commit.

`section-domain.ts` sequentially invokes the existing joint polygon solver/cache, checks complete C/C0/L, complex response and impedance eigenvalues with fixed floors (1fF/m,1pH/m,0.01ohm,1e-4 complex component), and independently requires physical-width, exterior-boundary and mesh sensitivity <=2%. Plans are capped at10 steps/300s/2GiB, with shared remaining time and cancellation checks. It preserves source/port/material identities; changed conductor sets/order, signal geometry/materials, incomplete/nonfinite output are refused. Retained audit comprises small observables/cache/resources, not intermediate mesh fields; existing bounded cache remains authoritative. Native request cache already hashes full cropped polygon/material/boundary geometry; full original board identity additionally binds the refinement audit. No new global cache.

`extractWideBoardSectionCoefficients` in segmented-channel uses authoritative board-section slicing and physical source-region/path/layer identities, then the helper. It does not call sectionDescriptor or turn cropped reference width into DC R or internal L. One/two signal admission only; additional conductors require explicit supported modal checking. This is an internal coefficients API; no wide-plane admission was added to the existing full RLC channel path.

## Coupon and independent accuracy

Manifest fixed before numerical execution: two64-sided round-wire signals radius10um, centers x=+-0.15mm/y=0.3mm, single35um thick finite reference with top y=0, homogeneous air, PEC;10mm extrusion,1/10/100MHz, common physical reference and50ohm ports. Original large field attempt exceeded the existing100000-node polygon-refinement cap; a smaller domain failed width sensitivity. Evidence retained, no limits raised or unchanged numerical failure repeated. Final changed plan: return halfwidth1/2/4mm at exterior halfwidth6mm, then exterior7mm, then area4e-8 to2e-8m2. The smaller case demonstrated why expansion is necessary, rather than being admitted.

Reference generator independently collocates exact image Green functions of line sources strictly inside the circular signals against unit-potential metal boundaries over an infinite grounded plane. It imports no FEM producer;24/32/48 source refinements give1.264e-9 coefficient difference and2.52e-13 off-collocation boundary residual. Absolute C is independently computed, then homogeneous Maxwell L and independent matrix-exponential four-port propagation. A preassigned3% engineering uncertainty covers reference/model/polygon approximation; this is not a rigorous bound. Finite-ground truncation and native outer-domain effects are separately checked. The existing stripline FD reference has incompatible geometry and was not reused.

Supervised packet: `debugging/p7-closure/1793434444860498300`. Independent maximum errors: C2.7270%, L1.1933%, invariant modal Z1.4416%, every complex four-port S1.3072%; maximum plus3%=5.7270%<8%. Mutual C/L and far-end S30 are nonzero, with |S30| about1e-4..1e-2. Reference and native response curves overlap. Native generic mode vectors may rotate within homogeneous TEM degeneracy; comparison uses sorted sqrt eigenvalues of C^-1 L, a stable impedance spectrum, rather than comparing arbitrary per-vector impedance labels. Absolute C/L reference checks remain separate from TEM velocity identity.

| Quantity | Final coupon |
|---|---:|
| Input metals / signals / material regions |3 /2 /1|
| Nodes / triangular elements |33188 /65046|
| Electric factor values |1265676|
| Numeric factor values bytes (8 bytes/value; excludes indices/workspace) |10125408|
| Native memory plan bytes |115589407|
| Cold sequence / same-input warm sequence |3.8573s /0.9044s|
| Cold stages width1/2/4, air, mesh |0.8374/0.5098/0.5179/0.6744/1.3175s|
| Final width max sensitivity / air / mesh |1.0680% /0.5234% /0.6188%|
| Sampled process-tree RSS / private peak |541536256 /586317824 bytes|
| Supervisor wall including cold+warm+startup |5.265s|

Warm statuses disk/memory/memory/memory/memory (bounded cache evicted the earliest entry); no new assembly. Supervisor used1536MiB OS cap,1GiB headroom reserve,120s internal plan and145s external cap. Peaks are20ms sampled lower bounds, include Node and cache; not solver-only live allocation. No whole-STM32 magnetic speedup claim follows.

## Board handoff and limits

B2 handoff `debugging/p7-wide-return/STM32G0_CAN_RS485.json` has `CHANNEL_CORRIDOR_UNSUPPORTED`, all `geometricIntervalsMm` and `wideReturnDomains` empty: full transverse physical-union copper/outline/bore events conservatively blanket every selected route run. This is a concrete upstream integration blocker: no certified homogeneous candidate exists to slice/admit, and manually drawing a rectangle would bypass B2. Real STM32 coefficients were therefore not claimed. Board adapter is implemented but not positively exercised on a B2 STM32 candidate. Future improvement must certify a finite influence domain without silently dropping remote geometry. NanoScope/Thunder have the same B2 limitation.

Finite-R current distribution, truncated wide-return DC resistance/internal L, distributed interfaces, end effects, multiple detached returns/floating conductors, fences/slots and residual inter-block coupling remain D/other C work. No global plane energy is added; no MHz PEC result extends lossy channel bandwidth.

## Verification and retained attempts

6 section-domain tests and2 existing field-cache tests pass, including cancellation, incomplete/nonfinite, actual material mismatch, physical/topology/axis change and unresolved sensitivity. Three selected actual FEM section/branch/coupled P6 regressions pass after correction of an accidental misplaced guard during editing. An initially broader segmented suite was stopped after it entered expensive unrelated local-volume controls; it is not reported as passing. One overlap of that suite and the selected small regression happened unintentionally; it does not affect the separately supervised coupon resource evidence, which finished earlier. Final typecheck/build pass; native/addon unchanged. Earlier transport refusals (`maxMemoryMib` spelling and conductorOrder location) and one transient syntax error have saved packets; these were adapter corrections, not numerical retries. No successful result is inferred from failed attempts.

Reproduce reference with `python tests/fixtures/analysis-p7-wide-section/independent-image-reference.py`; run `scripts/verify-p7-wide-sections.ts` under existing `verify-p7-closure-supervised.py` command override, then `python tests/fixtures/analysis-p7-wide-section/plot-and-compare.py <packet>`. Inspected wide-section.png; coefficient plot uses explicit pF/m and nH/m scaling and response/sensitivity plots retain failed early-width comparison. Stop for parent review.

