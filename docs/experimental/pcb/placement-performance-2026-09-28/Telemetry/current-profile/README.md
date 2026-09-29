# Current Telemetry timing, 2026-09-28

Full unchanged production policy at `64cef2f`, 154 components, 45 blocks.
One fresh process with the existing profiling harness; up to six native threads.
No compiler, second placement benchmark or test suite ran concurrently.
Background host load was not controlled. Timing includes diagnostic captures.

| Stage | Seconds |
|---|---:|
| Block/island native solving | 585.89 |
| Two full-board native packing calls | 84.40 |
| All 193 postrefine native calls | 17.72 |
| Remaining orchestration, scoring and capture | 12.97 |
| Total | 700.98 |

All final component poses match the previous global Telemetry after result.
Placement validation passes. Full-board postrefine itself took about 4.95 s;
the rest of the postrefine time was inside blocks.

The main block batches are current_iso (203.09 s, eight hypotheses), adc
(143.52 s, five hypotheses), voltage_iso (115.49 s, eight hypotheses).
Together they account for about 79% of native block solve wall time.
No pair-stage candidate from these three blocks entered their final portfolios.
Their retained results came from beam/postrefine or singles.

The earlier architecture capture with the same input SHA recorded legacy at
61.12 s (blocks 7.42 s) and full-micro at 71.08 s (blocks 15.96 s). These are
historical observations, not fresh controlled comparisons. The later global
run took 2040.45 s with concurrent workload and fewer configured threads.
Do not attribute its entire difference from this run to parallelization alone.

Next proposed experiments, not implemented here: a shared dependency-aware
worker queue across blocks and hypotheses; screening all hypotheses before
running expensive pair refinement; avoiding repeat refinement of identical
checkpoints; stronger incremental scoring; sharing work between ordinary and
alignment board searches while retaining both candidate sets. A two-minute
budget requires roughly a 5.8x reduction from this run; it is not achieved.

`timing-analysis.json` records input/binary hashes and per-block times.
`summary.json` and `result.json.gz` are unmodified outputs of the existing
measurement harness. `native-profile.log` records beam/singles/pairs durations;
summing worker durations is not the same as wall time.
