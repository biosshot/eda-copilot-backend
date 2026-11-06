# Parallel profile and explicit source candidate state

`P7_MAGNETIC_PROFILE=1` independently enables the existing adaptive phase
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
