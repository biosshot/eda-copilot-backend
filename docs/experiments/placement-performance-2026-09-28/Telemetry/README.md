# Telemetry placement performance

- `current-profile`: measured full placement at `64cef2f`, 700.98 s.
- `staged-final`: measured full placement with staged expensive refinement,
  476.10 s. All final component poses and independent metrics match the baseline.
- `staged-comparison/comparison.html`: full-board and seven-block comparison,
  with assembly JSON and timing details.
- `isolated-block-metric/comparison.html`: separate micro/geometric experiment
  on frozen block inputs. Neither board packaging nor post-refine runs here.
  Do not compare its aggregate time directly with a full placement run: skipping
  post-refine also changes which hypotheses qualify for pair continuation.

The earlier `staged` and `staged-v2` runs were interrupted after detecting
quality regressions. They are debugging captures, not final measurements.
`staged-block-check` reuses exact initial native solutions from the same binary
and checks repaired checkpoint selection on three heavyweight blocks. It is a
correctness replay, never a timing result. Native/input hashes accompany captures.

The isolated comparison does not change the production `routingMetric: micro`
policy. Its geometric mode has no A*; common geometry, pad-crossing and electrical
checks remain active. Frozen children isolate the parent solver comparison from
recursive child-layout changes.
