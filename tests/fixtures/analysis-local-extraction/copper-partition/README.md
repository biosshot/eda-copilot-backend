# Copper partition / near-cost audit
CPU F64, immutable saved inputs, no production mesher/kernel/API changes.

The completed packet is under ignored debugging/p7-design-checkpoint/
copper-partition/. Do not repeat its one completed measurement batch.

Reproduction order (coordinate heavy slot first):
1. trace.py reads accepted CAD/Board/preparation/source contours/supports.
2. run-batch.py build compiles actual unchanged sources plus a telemetry-only
   copy, using cached geo/serde_json libraries, codegen-units1; no addon build.
3. run-batch.py plans exports actual native theta0.4/leaf8 membership, no kernel.
4. census.py classifies geometry and freezes measurement-preflight.json.
5. run-batch.py run permits ONE120s/512MiB process, no completed retries.
6. audit.py, coordinate-audit.py, trace-failure.py, export-tables.py,
   report.py, plot-geometry.py and plot-cost.py use saved data only.
7. test-audit.py checks evidence without kernel execution; manifest.py hashes.

Selection and rules precede all measurements:28 unique samples from contact/
false-near cohorts, axial controls, per-family tails and reused STM witnesses;
XY12/z8 and24/z16. Sample positive fractions are point-query telemetry, not
population fallback estimates. Shape-bin gaps remain unpriced. Failed STM613
self is retained at both orders, never assigned a cheap successful cost.

The sole recommendation is existing individual moment checks streamed inside
coarse near leaves, with full aggregate physical energy budget. It keeps
geometry/current space/self/refusals. This audit does not implement it.
