# P7 joint channel-group runtime, 2026-11-04

The existing `.si().channels([...])` now dispatches to joint physical extraction.
No new DSL selector or P11 waveform/timing solver is introduced. The source
waveforms and requested duration determine spectral demand through the existing
analytic source-tail planner. Without those inputs the request is explicitly
rejected rather than assigned an arbitrary frequency range.

One extraction retains all selected channel/differential legs, original physical
return contacts and deduplicated handle mappings. Driver/receiver handles must
belong to the selected group. They describe demand and future loading; their
loads and stored state are not silently included in the extracted passive board.

The full requested interval is retained, including DC and the high-frequency
tail. The current segmented applicability is 1–10 kHz, further intersected with
supplied component bands. Only that intersection is sampled. The output records
uncovered intervals and remains `partial/unverified`; no waveform or timing is
returned. Empty intersections return the explicit gap report without a solver
call. The automatic foil callback is available for eligible lateral planes.

The live DSL result carries the frequency-only resource as `solution.network`.
The existing persisted-report resource scrubber records its identity and digest,
with physical mappings, source demand and evidence alongside it. This is P7
preparation for DDR/group analysis, not positive full-band DDR acceptance.

Verification: the focused live multiport/automatic-foil tests passed (8 tests),
as did typecheck and package build. The new group test traverses actual DSL
dispatch and captured geometry with one substituted joint numerical result. It
checks deduplicated handles, GHz demand versus explicit low-band gaps, missing
source inputs, live resource reuse and persisted identity. Independent mutual,
crosstalk, group-current and real-board accuracy are still open.
