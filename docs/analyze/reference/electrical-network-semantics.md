# Explicit electrical network (P6a)

This is an explicitly given lumped network, **not PCB frequency extraction**.
The existing Analysis DSL/process/shared addon calls the CPU/F64 Rust MNA module
`native/analyze/src/electrical`. Board/conductor FEM DC keeps its existing path.
No new public runner, addon, dependency, compute runtime or placement change.

## Physical semantics

- V=V(first node)−V(second node), current first→second. Passive power: consumer
  positive, delivering source negative (Velement × Ielement). Quality sourcePowerW
  and sourceWorkJ denote delivered source power/work, i.e. the opposite sign of
  the aggregate absorbed source power. Losses contain R/ESR/DCR dissipation,
  not stored energy. Explicit reference (or documented gnd) is mandatory; no
  automatic ground joining, shunts or gmin. Singular constraints reject.
- Local R/C/L models use exactly `pins:{p:'node',n:'node'}`. Resource/nonlocal,
  temperature-dependent and nonideal source models reject. Explicit ideality maps
  absent parasitics to zero; explicit zero ESR/ESL/DCR stays zero. No hidden values.
- DC uses constant voltageV/currentA, C open, L only DCR. Time initial('steady')
  uses absolute waveform values **at t=0 on the right**. Waveform is never added
  to the DC constant. Mode, request and initial policy remain in the report.
- Ports are passive observations. Late circuit.port() reads saved node fields.
  current('elementId') returns branch-oriented current; current(port) has no
  unique branch and is unavailable. Standalone spatial maps are unavailable.
  Board host input never automatically connects PCB copper to a circuit.
- AC Z injects +1 A into positive, extracts at negative, and uses Vpositive−Vnegative.
  Independent V=0 and I=0. Every port is a separate excitation with factor reuse,
  not a matched termination. DC constants/waveforms are not AC amplitudes.
  AC voltage/current/transfer/resonances remain explicitly unavailable.
- Z is a complex Ohm series on frequencyHz/Hz. real/imaginary/magnitude/phaseDeg
  preserve axes. Phase atan2 is [-180°,180°]; zero phase is undefined/null with
  conditional validity. Complex min/max requires real/magnitude first.
  at() requires an exactly saved axis point, without interpolation/extrapolation.

## Numerics and identity

One compiler retains sparse G/E incidence. Branch unknowns impose V-source/L
constraints; C with series ESR/ESL has a branch current and capacitor voltage.
DC solves Gx=b; AC uses real-block G+jωE; time solves the equivalent increment form
(G+E/h)delta=b−Gx_previous, x=x_previous+delta, avoiding cancellation of large
capacitor charges E*x/h when recovering small rail currents.
`faer` sparse symbolic/numerical LU, no inverse or dense production solve.
One scenario retains at most one compiled model, symbolic pattern and factor.
Context/topology/R/C/L invalidate compilation; RHS-only changes reuse identical
factors. Frequency, h and constraint changes invalidate numerical factors.
Compilation/symbolic/numerical/solve/reuse/accepted/rejected counts are observable.
`solves` counts primary RHS requests; each includes two iterative-refinement
correction solves with the same factor (not additional primary requests).

Backward Euler compares a full step with two half steps; accept the two half
steps if every component's normalized error≤1. Defaults: relativeTolerance=1e-6,
absoluteTolerance=1e-9; the latter multiplies **1 V for node/capacitor voltages**
and **1 A for branch currents**. Relative scale is max(|old|,|new|) of that same
quantity. V/A never share a raw epsilon. Both tolerances must be positive≤0.1.
maxIterations is the attempted-step budget (default200000, maximum1000000).
DC/AC custom numerics reject; fixed guards are not silently overridden.

sampleStepS controls output only. Piecewise-linear dense output uses accepted
half steps. Internal steps are seeded at start/source corners by conservative RC/LR/LC
scales, avoiding false acceptance of two equally over-damped large BE steps.
Adaptive steps separately resolve source formula boundaries, finite
fronts, repetitions/count endings; sine is additionally capped at period/50,
general step at interval/20. Formula widths below 32 F64 epsilons of their
absolute event time reject explicitly rather than being skipped. Changing output cadence does not determine dynamics.
Sampled extrema may miss peaks: use sufficiently fine output for peak checks.
At instantaneous current/PWL jumps integrate to the left, preserve C voltage and
L/ESL current, then solve right-hand algebraic constraints. Exact-event output is
right-sided. Nonimpulsive ideal V steps (e.g. through R into C) are supported. An event
incompatible with preserved C/L state, e.g. an ideal V jump directly across an
ideal capacitor, rejects NETWORK_IMPULSE_UNSUPPORTED: specify a finite front.
Contradictory initial constraints reject without changing initial energy.

Residual (including the incremental time system) is row backward error |Ax−b|/(|b|+Σ|Aij*xj|), physical equation floor
1e-15, guard1e-8. KCL is separately reported in A (both AC quadratures). DC/AC
power guard1e-12W/VA +1e-8 actual exchange. AC peak-phasor power uses a common
convention on both sides (a factor1/2 cancels). Signed errors are never clamped.
Time work/loss uses endpoint quadrature on every accepted BE half step.
EnergyErrorJ=sourceWork−dissipation−Δstored. Normalization is
∫(|sourcePower|+loss)dt, floor1e-15J. Error includes BE numerical dissipation;
RLC refinement tests show reduction rather than calling it physical ESR.

finalState has context/model identity, physical time, complete MNA vector and
explicit C voltages/L currents. The bounded session recognizes its **latest**
produced state. Unknown/modified/displaced/foreign/changed-parameter states reject.
DC/AC on the same model do not erase it; another compiled model replaces it.
No unbounded state archive or arbitrary transfer. stopS is absolute and must
exceed state time; first output is the saved state, phases/events never restart.

Limits:1024 nodes,2048 elements,128 ports,100000 output points/port-frequency
solves. Before large allocations, conservative planning bounds sparse fill,
real-block workspace and output arrays. Default256MiB, explicit≤4096MiB.
maxSeconds(default120) is checked between frequencies/attempted time steps.
Existing process deadline/abort interrupts synchronous native work and awaits
child exit, without nested processes. Memory is a plan, not a hard RSS ceiling.
Report OS high-water worker RSS through native calls includes Node and earlier
calculations, but excludes later final serialization. A numerical/budget failure
never produces successful zeros; the next public runner remains usable.

## Direct real-board scenarios

`npm run test:pcb-analyze -- <fixture> --name rlc` runs exact authored rlc.js.
STM32G0_CAN_RS485, USB_PD_Trigger_5A and ThunderF722 retain their existing dc.js.
Each RLC run verifies pad/net bindings, compares two test load/ESR modes, computes
Z/phase/transient/current, refines time tolerance, continues state and keeps a
strict failing check followed by a passing check and plots.
The launcher explicitly requires the current indexed DC evidence for the same
board and shared/FEM implementation hashes, re-audits and copies its self-contained report. An isolated RLC-module-only rebuild
may have another addon hash: both binary versions are retained, and every shared/DC
source hash is checked against the executed DC snapshot. Unchanged FEM is not
replayed cyclically for an unrelated electrical-module change. No searching
old successful runs to fill missing current fields. Assertions live in saved
tests; generated JSON/HTML/SVG/provenance lives only in ignored debugging/.

The saved extreme 1 ps RC / 1 kA test at t=10 ms is deliberately retained: the
former large-step heuristic falsely completed with 50% energy error. The new
physical seed explicitly rejects this F64-unresolvable requested current/time
accuracy (NETWORK_STEP_UNDERFLOW); a 1 ns / 1 A reference completes with the
physical capacitor/Ohmic energy balance. No floor or physical input is retuned
to hide the unresolved case.
