# Electrical core: supported completion scope

P6 implementation, CPU/F64; numerical/package acceptance is recorded separately
in [verification](electrical-core-completion-verification.md). Existing PCB DC,
P6b frequency tables, P7 cross sections and placement/GPU retain their contracts.
Using the existing `si().network(...)` configuration exposes supplied temporal
models only. It does not implement PCB channels, TDR, eye or timing.
Start with the [executed user examples](electrical-core-examples.md) for source
spectra, finite drivers/receivers and explicit sampled-model closure.

## Circuit models and AC

`pi().circuit(c).domain('frequency')` accepts existing `.sources/.loads`.
Sources require a complete `voltageSpectrum` or `currentSpectrum` on exactly the
requested frequencies, V/A, peak complex phasors with `exp(+jωt)`. DC constants
and time waveforms are never inferred as AC amplitudes. Existing independent
circuit sources are zeroed for both AC and separate unit-current impedance tests.
Explicit spectra produce a separate response, with the actual finite source R
and passive loads. AC complex power is checked independently of the solve.

`voltage(port)`, element `current(id)` and `transfer({from,to})` retain complex
series. Transfer means V(to)/V(from), requires exactly one active explicit source,
and retains null/unavailable values at zero denominators. It is not S21.

`linear-driver` is authored absolute voltage behind a positive output R; low/high
map bits/PRBS levels, while Waveform is already absolute. `rc-input` is a positive
input C with optional parallel positive R. No clipping, automatic switching,
regulator feedback, general controlled-source family or component parasitics are
inferred. Current/power load limitations remain explicit; constant-power models
are not silently linearized.

An explicit `kind:'model'` circuit element accepts ComponentDescription with
`pins:{p,n}`. Parameter precedence: authored values/model (conflicts reject),
exact board designator nominal, exact supplied PartUuid catalog. No board/BOM/
network lookup is required. Strict SI values/embedded resistor forms are admitted;
bare capacitor/inductor codes such as `104` reject. ESR/ESL/DCR require explicit
values or explicit `ideal:true`; a nominal never supplies them. Resolved inputs
and their source are saved.

## Conditional sampled-to-time

Ordinary S/Z/Y/Touchstone frequency use needs no temporal metadata. Optional
`data.temporal` explicitly authorizes the restricted relaxation assumption:

```
method: 'passive-relaxation'
modeTransform?: real orthonormal U
normalizationOhm?: positive scalar / exact named port map
highFrequency: {conductanceS: gInf[], capacitanceF: cInf[]}
Y(s) = U diag(g0 + s*cInf + Σ h_k*s/(s+p_k)) Uᵀ
```

Known finite real DC at f=0, passive samples, 1–4 ports, ≤4096 rows, positive
frequency gaps ≤0.1 decade, constant modal U and explicit high-frequency closure
are required. `gInf` is the conducting remainder after subtracting `s*cInf`.
`g0,cInf,h>=0`, `p>0`, `Σh=gInf-g0`; no pole reflection, passivity repair or missing
DC/extraband coefficients are invented. The realization is physical parallel G/C
and series RC branches. Stable poles alone are not the passivity argument.

Starting from 8 logarithmic poles in `[10*2π*fMinPositive, 2π*fMax/10]`, bounded
training-only enrichment adds four geometric midpoints per iteration, ≤64 poles.
Constrained active-set NNLS checks feasibility and normalized KKT≤1e-9; unresolved
conditioning fails. Odd rows are fixed holdout data. All normalized coefficient
errors at training and holdout rows must be ≤0.005+1e-9 and final reported
8/16/32/64 model-order change ≤0.001. Physical Y and fitted Y are saved separately.
The normalization `sqrt(Ri Rj)*ΔYij` is authored for Y/Z; S defaults to its original
real port references or an explicit normalization override. Conversion always
uses the original S references. U transforms V/I together and preserves power.

Fastest declared source front must be ≥10/fMax. Initial RC state is the complete
assembled steady state, including finite drivers/receivers. Time error and model
error are different checks. The result and derived values remain **conditional**:
finite samples, a regular grid and holdout errors do not rule out unseen narrow
resonances or prove original intersample/extraband behavior. General resonant,
active/noncausal fitting, arbitrary delay inference and lossy P7b-table-to-time
are unsupported. Successful approximation is not universal Touchstone-to-time.

## Full coupled descriptor and P7

Existing network Resource accepts `format:'coupled-rlc'` with ordered
potentialNodes/currentDofs and sparse full triples for A, B, R[Ohm], L[H], C[F].
General real A/B are dimensionless; duplicate/nonfinite/out-of-range entries,
asymmetric matrices and nonpassive storage reject. First supported R is positive
definite when current DOFs exist (finite-conductivity P7); L/C may be PSD/zero.

`C vdot+A i=B ip`, `L idot+R i−Aᵀv=0`, `vp=Bᵀv`.
Stored energy is `(vᵀCv+iᵀLi)/2`, dissipation `iᵀRi`. Mutual terms and all infinity
potentials remain. `referenceTopology:'infinity'` is a mathematical reference,
not an electrical return terminal; A/B columns conserve total charge. Explicit
candidate `conservedPotentialModes` span null(Aᵀ), are checked for independence,
and are reduced after external source/load assembly. Pure C defaults to all
potential modes. DC compatibility is tested against the assembled nullspace;
stored charge constraints differ from exact zero-energy mathematical gauges.
Time evolves charge under excitation and never forces each island neutral.

Direct single-block `networkParameters` evaluates the original matrices and
keeps the coupled Resource, B, reference topology, analyses and original validity
domain in `result.network`. It does not label arbitrary differential terminals
as a physical common return. Infinity/noncanonical descriptor composition rejects
until a physical terminal map is provided. Grounded canonical unit-potential
ports can compose by the explicitly authored node identifications; block R/L/C
are assembled before charge constraints/initialization. No geometry is extracted
here; P7 owns recovery, constitutive assumptions, scope and convergence.

Admission: ≤1024 potentials, ≤1536 currents, ≤2048 total states, ≤16 external
ports, ≤4 million input coefficients. Conservative matrix/gauge/output plans
must fit the requested memory, default256 MiB; caps do not imply every maximum
fits simultaneously. Original equation residual≤1e-8 and energy/power guards
remain mandatory. No gmin, hidden ground or PSD projection repairs a failure.

## Exact real-K delay models

`delayed-scattering` has real constant `scattering:K`, `delayS:τ>=0`, positive real
`referenceOhm`, passive nature and common return. It defines `S(s)=K exp(−sτ)`
on DC through high frequency; K is a contraction within1e-9. It is an explicit
analytic resource, never an inferred property of a finite table.

Power waves are `a=(V+RI)/(2√R)`, `b=(V−RI)/(2√R)`, with network current inward.
Positive delay uses accepted incident-wave history only; h≤τ/4. Zero-delay K
enters the same algebraic solve, including ideal-through constraints. Pairing
joins authored positive and common return terminals; all block ports must be
paired or external. Finite drivers and passive RC receivers load the network.
All-delay and mixed grounded canonical RLC/passive-relaxation cascades use one
assembled `(v, branch-current, incident-wave)` backend. RLC matrices retain mutual
terms and each source is validated before merging. Pure coupled general-incidence
behavior remains separate; infinity/noncanonical joins still require an explicit
physical terminal map and reject. No Y inversion, Pade approximation or inferred
return is used. Steady prehistory is solved with every K, source and load present;
physical nonunique steady states reject rather than silently erasing charge.

`portCurrentsA` is inward to the connected network, including receiver capacitive
current recovered from accepted BE derivatives; values between accepted endpoints
are interpolated. `lumpedEnergyJ` includes receiver C and full authored C/L;
`delayEnergyJ` is the exact rolling integral of accepted linear wave history.
`sourceWorkJ`, `resistiveLossJ`, `attenuationLossJ` and
`beNumericalDissipationJ` retain separate cumulative traces. Source and resistor
work use accepted BE endpoints; attenuation integrates the contraction quadratic
exactly across all history knots, including mutual K cross terms.

The fixed energy norm is `abs(Wsource)+WR+Watten+abs(DeltaUlumped)+
abs(DeltaUdelay)+Dbe`. `rawEnergyBalanceJ` is work minus physical loss/storage;
`energyBalanceJ` additionally subtracts only positive BE storage dissipation
`Dbe=0.5*delta^T E delta`. Its absolute value must be at most `1e-4*norm`
(or `1e-20 J` for exactly zero norm). `historyQuadratureDefectJ` is a signed
diagnostic from endpoint wave work minus exact history storage/attenuation;
it never corrects acceptance or becomes fictitious numerical dissipation.
Independent global time/history refinement remains mandatory. Mixed frequency graphs evaluate each grounded
canonical descriptor directly with positive wave-reference admittances at every
requested frequency, then reuse the existing wave composition. This avoids
singular Y/Z conversion for ideal through/shunt constraints; original Resources,
permissions and producer bands remain in provenance. General/infinity descriptor
single-block frequency semantics remain separate.

Initial travelling-wave history is solved as constant steady state before t=0;
it is not zero-filled. Arbitrary incoming histories/continuation are unsupported.
Finite source corners propagate by actual delays, with bounded event delivery;
history interpolation is piecewise linear over accepted half steps. Independent
global refinement is still necessary: local BE error does not certify delayed
interpolation. Bounds include≤128 internal ports/32 blocks/16 external ports,
2 million history numeric values,100000 delivered formula corners and an admitted
matrix/history/output memory plan. Budget exhaustion gives incomplete/rejected,
not a truncated complete trace.

Delay energy is `Σ∫[t−τ,t] ||a(s)||²ds`; propagation loss is
`||a(t−τ)||²−||K a(t−τ)||²`. They are separate from driver/receiver R and C energy.
K=lossless through has zero internal loss; a passive attenuator's loss is not
mislabelled as delayed energy. Source work/load/storage balance is checked.

## Initial conditions, output and limits

Frequency results retain a complete ordered single coupled/delay resource only
when its original physical port order is preserved. Renamed delay reference
dictionaries are remapped to the new port names. A reordered single-block delay
connection or a cascade produces a frequency-only sampled resource, bounded by
its evaluated output axis and all producer bands. Coupled joins/reordering remain
explicitly unsupported. Existing sampled frequency results retain this same
frequency-only boundary; temporal closure is not invented for a new sampled axis.
All source validity/analysis permissions remain in derived provenance. A producer
band constrains evaluation in addition to the resource's mathematical definition
or stored samples; successful evaluation never expands that band or permissions.
Repeated computed-resource reuse keeps direct-parent resource hashes and compact
validity snapshots; generated ancestor provenance/history is SHA256-linked rather
than recursively copied. Authored applicability, status/reason and permissions
remain visible at each direct parent. Saved earlier results retain the full
history represented by those links.

One native request shares its original deadline across all source fits,
normalization, composition, matrix compilation, initial solve and time steps.
Retained source/fit reports are reserved together before fitting; matrix and
output plans use the remaining memory allowance. Neither an additional block
nor a new substage obtains a fresh full budget.

An authored finite-band coupled model must explicitly authorize SI, include DC
and pass the finite-front screen (fastest declared front at least `10/fMax`).
Its result stays conditional: this screen does not establish absence of source
spectrum outside the band or extend the producer's constitutive applicability.

Descriptor BE uses increments and step doubling; sampleStepS selects output
interpolation only. All models require continuous finite fronts in this new
path; impulse/jump projection is not promised. Bits start at their first level
at t=0 and hold the last after completion. PRBS7/9/15/23/31 use documented taps
6/5/14/18/28, MSB before left shift, no inversion; unsupported orders reject.
Waveforms/PRBS do not imply BER, eye masks, jitter or statistical validity.

Current PCB extraction results only gain time applicability when their producer
explicitly authorizes it with justified DC/band/constitutive assumptions. Neither
the descriptor's mathematical time solution nor a small residual proves field
extraction accuracy. Remaining limits and future P8/P11 dependencies are listed
in the verification matrix; the original P6 stage is not labelled universally
complete from these supported subsets.
# Physical conductor composition extension (2026-10-24)

Full unreduced coupled RLC Resources may declare `data.portTerminals` in
Resource.ports order, each with positive/negative `{node: potentialNodeName}`.
Only grounded Resources may explicitly author `{reference:'ground'}` as a
physical return. The exact B column must equal positive minus negative trace.
Existing `a.connect` welds both contacts by default; optional connection
`contacts:'positive'|'negative'|'both'` requires explicitly mapped coupled
endpoints. A one-contact weld does not constrain the other conductor or imply
equality of differential port voltages. External ports shorted by weld reject.

The existing coupled composer applies conductor congruence to full C/A/B,
retains independent branch R/L blocks and recovers original conductor names.
It does not extract absent intermodel mutual capacitance/inductance. Physically
close objects with substantial mutual coupling require a jointly extracted
model; a wire weld is not EM extraction. General B/reduced traces and delay
ports without physical conductor/return/charge trace remain unsupported for
physical composition. Legacy grounded/common-return graphs remain supported.

SI `.initial({kind:'physical-state',steady:false,potentialsV:[{instanceId,node,
value}],currentsA:[{instanceId,dof,value}]})` supplies every retained node and
branch DOF after topology selection. Welded potentials must agree; snapshots
are not averaged. Explicit initial state allows an unforced passive decay
without drivers/receivers. `steady:true` must satisfy the assembled steady
equations. Floating physical charge cannot silently default to zero; supply
the complete physical state. Only proven zero-energy null modes are gauges.
`numerics.maxStepS` is a coupled descriptor SI step cap independent of output
cadence; other electrical backends reject this option.

Reports retain conductor recovery, potential/charge traces, initial/final
charges, gauges, raw work-loss-storage balance and separate BE dissipation.
The energy exchange gate requires both the legacy `1e-14 + 1e-7*norm` bound
and the normalized `1e-4*norm` bound (exact zero norm uses1e-20 J), with
nonnegative physical/numerical dissipation. A tiny late transient exchange on a large charged rail may be
unresolvable in F64 and explicitly reject with `COUPLED_ENERGY`. No energy
floor relaxation, hidden ground or charge reset makes that continuation pass.
Physical-state seed, contact selections and recovery enter continuation
identity. Analysis remains CPU/F64; placement F32/GPU is unchanged. This
extension does not close general P6, P8 or P11.

# Authored resonant Y temporal admission (2026-10-24)

The separate explicit temporal discriminator is:

```js
{format:'network-parameters', representation:'Y', referenceTopology:'common',
 nature:'passive', frequenciesHz:[0, /* increasing positive samples */],
 parameters:[/* full complex Siemens matrices */],
 temporal:{method:'passive-resonant-y', normalizationOhm:50,
   modeTransform:[[1,0],[0,1]],
   dcSlopeF:[1e-9,0.25e-9],
   highFrequency:{inverseInductancePerH:[1e5,4e4]}}}
```

One or two ordered ports; exactly one series-RLC section per authored modal
column of constant real orthonormal U. All fields above are required; no default
U, inferred closure, alternate S/Z/Touchstone representation or automatic class
recognition. dcSlopeF is lim(Ymodal(s)/s) in farads; inverseInductancePerH is
lim(s*Ymodal(s)) in inverse henries. Both are strictly positive per mode.
normalizationOhm is one positive scalar numerical scale, not a physical load.
U acts on unnormalized physical voltage/current coordinates and preserves power.
The claim Y=U diag(k*s/(s*s+a*s+b)) U^T is authored, not proved from samples.

Fit only positive training rows (alternating rows; remaining rows are holdout)
using column-scaled reorthogonalized QR of 1/(Rnorm*Ymodal) in [1,z,1/z],
z=s/omegaRef, omegaRef=2*pi*sqrt(fminPositive*fmax). Positive coefficients
convert to physical R/L/C without clipping or pole reflection. Check closure,
DC=0, reciprocity, modal off-diagonal error, sampled passivity and full matrix
training/holdout residual <=0.5% +1e-8 normalized absolute floor. At most4096
rows, >=24 rows, <=0.1decade gaps, >=8 training samples per fitted resonance
linewidth, column-scaled QR condition<=1e10. Fit has fixed physical order2 per
mode, no inferred extra branches or order escalation. The frozen checkpoint's
higher-order refusals and coarse2ns energy failures remain historical evidence.

Export A=[U;-I], B=[I;0], internal diagonal C, branch positive diagonal R/L
into the existing grounded coupled descriptor. Execute existing AC/time assembly,
steady initial-state solve and adaptive BE; retain all production guards and
budgets. Source front >=10/min(sampled fmax,authored validity upper band) is only
a conditional screen. All SI results and derivatives remain conditional, retain
original Resource/provenance and band; composed sampled frequency outputs do
not acquire an inferred temporal contract. Ordinary frequency samples and the
existing passive-relaxation method retain their prior meaning.

Unknown intersample/out-of-band resonances remain unexcluded: the frozen hidden
Q=1e7 coupon demonstrates this limit. Parallel-Z ideal-L export, universal VF,
noise repair, delay inference and arbitrary frequency-dependent modal bases are
unsupported. This extension does not close all P6 or P11.
