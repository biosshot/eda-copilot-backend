# P7 experimental section-current integration

`P7_SECTION_HINTS=1` connects actual section recovery to the native mixed caller. Default is disabled. This is a current-basis predictor, not replacement of the global volume magnetic operator or completed 2D/local-3D decomposition.

The producer accepts conforming uniform Z extrusions with physical conductor ownership and conductivity preserved through every slab, unchanged dielectric distribution, common physical negative-terminal component and port contacts on both axial end faces. Source labels do not weld components. At most 32 real/imaginary current profiles are admitted. Other geometry uses the original path. The producer reserves bounded transient storage and at most one eighth of remaining memory/time; failed optional recovery falls back while a global deadline still refuses.

`solve_mixed_grid` checks profile dimensions/finite values and reserves live input plus additional modes before constructing operators. Profiles are R-orthogonalized after full DC restoration, before the initial magnetic action. The physical field, original-cell magnetic operator and original-space residual gates remain in use. Evidence reports submitted profiles and added current modes. The TS response cache distinguishes the enabled policy from baseline.

## Actual integration control

A board JSON fixture passes through the real board mesh builder, certified electrodes, section producer and mixed solver. The enabled process additionally exercises the native caller's flag dispatch. Two profiles add two current modes. Seeded and plain complex S entries agree within 1e-8 absolute; both retain original KCL/KVL admission. Nonuniform conductivity, mismatched reference and incompatible side contacts produce no hints.

| Process | Seeded solve | Plain solve |
|---|---:|---:|
| Enabled native dispatch | 8.982 s | 8.671 s |
| Default dispatch | 9.225 s | 7.415 s |

These are small debug controls, not a stable performance benchmark. They show no acceleration. KCL is about 1.11e-16 for both; KVL is about 2.38e-9 seeded and 6.06e-9 plain, below unchanged gates. Logs: `debugging/p7-seed-enabled.log` (one test, 28.25 s, three separately bounded solves) and `debugging/p7-seed-disabled.log` (one test, 16.70 s).

Seven TS cache/validation tests and typecheck pass (`debugging/p7-section-hint-cache.log`, `debugging/p7-section-hint-typecheck.log`). The validation helper was changed to a function declaration so TypeScript recognizes its `never` return for existing optional-certificate narrowing; runtime rejection rules are unchanged.

Release addon build passes (`debugging/p7-section-hint-native-build.log`).

## Decision and remaining work

Keep the predictor experimental and disabled. Adding current modes can increase initial magnetic work, and this experiment does not justify enabling it for STM32. No heavy board rerun is warranted for this mode. Actual cheap homogeneous sections, distributed cut-potential interfaces and reduced global-return field work remain to be integrated; the entire case-matrix remains required and its acceptance is not promoted.

An ignored `synthetic_near_profile::profile` diagnostic is also preserved in `polyhedron.rs`, with a 30 s internal deadline. Three thin synthetic tetrahedron pairs in debug use 19,224 analytic evaluations and no positive fallback (`debugging/synthetic-near-profile.log`). This is reproducible diagnostic infrastructure, not a measurement of actual STM32 pair hotspots or an implemented kernel acceleration.
