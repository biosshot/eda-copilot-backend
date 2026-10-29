# Finite-source spectral dependency

Source plans use the actual continuous finite event train, slope variation, duration/repetition and explicit input-tail tolerance. Output sample cadence and baud alone do not define applicability. Continuous PWL has two-sided Fourier L1 tail at most TV(x')/(2*pi^2*F); unilateral sine includes onset. Ideal jumps and discontinuous repeated boundaries remain unavailable. This is an analytic input bound, not an output-percentage certificate.

For finite Thevenin source resistance Rs and resistive receiver Ro, passive available power bounds each voltage transfer by sqrt(Ro/Rs)/2. The difference of two passive responses above F is bounded in absolute volts by sum(sqrt(Ro/Rs)*inputTail). The report requires DC coverage, corresponding zero-energy starts and zero source prehistory. Pure capacitive receivers, unknown stored energy, missing source bands and overflowing bounds remain unavailable. In-band extraction, spatial/model differences and time integration error remain separate.

The public transient adapter retains source/resource hashes, plans and absolute output-tail bounds without relaxing native admission or conditional status. No new P8/P11 product analyzer is introduced.

## Saved evidence

`debugging/p7-channel-plan/tps-source-spectrum-1793282899727` contains the actual direct DSL and report using the original accepted TPS finite-ROI RLC under an explicitly authored developer DC-extension hypothesis. It is a compatibility/provenance case, not production DC-band admission:1mV source,200us continuous ramp,1000Ohm source and receiver,1pF receiver, unenergized physical state. At100kHz the input and passive response-difference HF-tail bound is5.066059microvolts;1% input-tail demand is50.660592kHz. The numerical loaded limit reaches0.5mV; energy-balance error3.50e-17, recorded process high-water RSS115679232bytes. The generated source-band/response PNG has been visually inspected. It labels the explicit hypothesis and remaining in-band/spatial/time gaps.

The independent actual requested low-frequency full/reduced TPS15/15 journal is documented in `p7-channel-plan-checkpoint.md`. Neither it nor this source-spectrum case alone excludes untested intersample poles or supplies a complete physical5% transient certificate.

Focused tests: six source-spectrum tests and two existing transient-adapter tests; TypeScript typecheck/build passed. Numerical thresholds and production extraction band remain unchanged.
