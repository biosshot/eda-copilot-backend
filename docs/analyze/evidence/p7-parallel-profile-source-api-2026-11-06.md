# Parallel profile and explicit source candidate state

`P7_MAGNETIC_PHASE_PROFILE=1` independently enables the existing adaptive phase
profile, retaining the historical `P7_MAGNETIC_TRACE` record prefix. Unlike
`TRACE=1`, it does not enable serial near-kernel diagnostics. Completed plan
summaries record traversal counts, elapsed planning time, cache enabled state,
entry count, retained capacity bytes and admitted cache ceiling. Partial phase
checkpoints survive a native deadline. Writes are bounded and nonfatal;
profiling timers add overhead, so profiled timings are diagnostic rather than
unprofiled throughput measurements. The STM32 identity records the new flag.

The isolated profile-off/profile-on control uses four near workers in both
processes. It requires identical action and bound bits, equivalent complete
certificates after removing only timing fields, and an actually configured
four-worker pool with profile enabled. It passes under an external 30 s /
2048 MiB supervisor in 0.250 s in
`debugging/p7-parallel-profile-isolation-controls/`.

The first proposed diagnostic name conflicted with the existing TypeScript
`P7_MAGNETIC_PROFILE=baseline/compact-v1` compute policy. The 10 GiB attempt
was refused before launch by commit-headroom admission; the 8 GiB attempt
stopped in configuration validation before any numeric solve. These artifacts
remain in `debugging/p7-phase-profile-cover-cache-stm32-250s-10g/` and `...-8g/`.
The diagnostic is renamed `P7_MAGNETIC_PHASE_PROFILE`; the compute-policy
contract is unchanged. The planning-only configuration preflight passes with
zero solves in `debugging/p7-phase-profile-config-preflight/`. Renamed isolated
off/on controls again pass in 0.250 s in
`debugging/p7-phase-profile-renamed-isolation/`.
The corrected native addon build passes in
`debugging/p7-phase-profile-renamed-native-build.log` (`BUILD_EXIT=0`).

## Callable source-only shared-return candidate

`section_return::solve_source_candidate` builds a fresh source R/L/C model from
the validated basis and supplied inductance. It accepts real/imaginary port
drives and derives their original physical electrode injections. It returns
`CandidateState`: original high coordinates, separate compensated coordinate
corrections and full compensated nodal charges. Port projection preserves both
parts and rejects malformed or nonfinite state.

Before returning, the API checks the original assembled complex equations and
the original physical charge/KCL/KVL gates. Its compensated KCL uses charge low
parts; the old `solve_candidate` retains its existing rounded computation.
Physical ownership lookup is linear to construct, avoiding a component-node
membership scan for every coordinate. Budget admission includes simultaneous
Space/Field/magnetic operators, basis/model/System, indicator responses,
scratch and state. Construction and operator boundaries check the deadline;
individual operator calls also need their own bounded implementations.

The six-prism shared-return control calls this API for two drives at 1/10 kHz
with the order-16 volume reference. All four pass unchanged gates; maximum
normalized port difference is 2.715e-13. An altered coordinate and a separately
stamped positive admittance both fail the assembled-equation audit. Invalid
ports, insufficient memory and zero deadline refuse. The order-8 reference
continues to pass at 1 kHz and refuse at 10 kHz; that sensitivity is retained.

This is a private **source-only** API: a previously stamped System or attached
devices are not accepted as inputs. It is not an automatic board route and
does not admit component bridges, all case-matrix families or a larger band.
The original automatic path is unchanged; no device or source is silently
discarded by this addition.

The combined external 120 s / 2048 MiB release supervisor passes 38 controls in
9.578 s in `debugging/p7-parallel-profile-source-candidate-controls/`. Sampled
peak private/RSS is 25,804,800 / 38,682,624 B (20 ms lower bounds).
The native addon build passes in
`debugging/p7-parallel-profile-source-candidate-native-build.log` (`BUILD_EXIT=0`),
and TypeScript typecheck passes after recording the numerical profile flag.

## Rejected orientation shortcut

A separate external 30 s / 2048 MiB captured-pair diagnostic completes 16
original bit-exact pairs, two repetitions per direction. Sources are prepared
outside timed loops. Original direction totals 0.0700229 s; reversal totals
24.9926174 s, about 357 times slower. Maximum relative directional difference
is 1.904e-12. The higher-order reference is explicitly skipped when less than
ten seconds remain; this is not independent all-case accuracy certification.

Output: `debugging/p7-adaptive-orientation-captured-controls/`, wall 25.265 s,
sampled peak private/RSS 11,509,760 / 27,418,624 B (20 ms lower bounds).
No production orientation change is selected. The result rules out this simple
shortcut on these captured pairs, not every possible geometry.

## Actual parallel STM32 phase profile

Corrected code `bc9e0e5`, output
`debugging/p7-phase-profile-cover-cache-stm32-250s-8g-renamed/`: native 240 s /
8192 MiB, external 250 s / 8192 MiB tree cap; compute profile `baseline`,
phase profile enabled, four near workers, axial mode, one RHS per cover,
four reduction iterations, cover cache 1024 MiB. Trace, scalar cache and
near-bound omission are unset. The identity records all flags. Planning retains
844 authored copper regions, 65 islands and 89 barrels; full physical acceptance
remains unverified. The initial refusal and configuration failure above did not
perform this calculation.

The native deadline occurs inside streamed magnetic action at external wall
243.719 s. Sampled peak private/RSS is 2,959,130,624 / 2,045,042,688 B (20 ms
lower bounds); memory is not the stopping condition. Native core identity is
`364eadd7580d8e2ed2060259d6d896387539ed045d9eaa95e3f72f6c2a7ca251`;
executed DSL SHA256 is
`817e427e109f58e50b21267a98cec5b4ac4322c2b37c1445e9e1b471bc967b61`.
Board SHA256 remains
`0159c43d43b59df4f59fb943d59f0d841d17ca38d6d5a7d5576920a4aee3cce3`.

The first magnetic batch lasts 124.231 s before refusal: geometry 5.214 s,
planning 31.076 s and the unfinished first RHS 87.896 s. Approximately 119 s
of external wall precedes this batch; this profile does not subdivide that
earlier work. Phase times are inclusive: do not add nested batch/axis/replay
durations. The plan performs 39,743,535 node-pair visits and yields 15,604,494
near tiles, 3,531,639 far blocks and 739,382 zero blocks. Classification alone
takes 25.069 s inside the 30.875 s cover walk.

**The cache is actually admitted and used:** 19,875,515 terminal entries,
capacity 33,554,432, retained allocation 1,073,741,824 B. First-axis replay has
zero classification time. Its unfinished 86.591 s contains 77.990 s near
callbacks and 8.539 s far callbacks, with 138,820 near tiles and 85,185 far
blocks visited. Fewer than 1% of the planned near tiles of the first axis are
reached; no complete first axis, RHS, magnetic action or channel is produced.

Committed kernel windows contain 138,752 pairs; summed worker separated and
adaptive elapsed are 163.064 / 74.337 s, not OS CPU seconds. Geometry builds
are 132,258; lazy preparation builds only 4,572 direct rules / 2,191,976 points.
Outer retries accept 54,100 of 58,645 attempts. Partial kernel reports can say
`failed:false` because their windows completed before the enclosing traversal
deadline; they do not establish action success.

The cache removes repeated classification during replay, but no overall
speedup or <=600 s completion is established. Profile timers, cache settings,
host conditions and code differ from earlier prefixes. The dominant observed
replay cost is the near integrals themselves, not repeated tree classification.
Next structural work must reduce costly near work while retaining all physical
sources and original current coordinates; virtual geometric refinement is a
candidate to measure before any production representation change. All automatic
shared-return and case-matrix completion requirements remain open.
