# Block hypothesis parallelism and path diagnostics

[HTML comparison](comparison.html) contains full ESP32-C3 boards, local blocks,
an assembly JSON and the saved ESPower board with signal-path guides restored.

## Retained change

Independent grouped, released and role hypotheses are sent to a bounded native
thread pool. Every hypothesis and native checkpoint is retained in the original
input order. Cached individual results remain reusable; only unique cache misses
are sent to the batch API. Older native addons fall back to serial calls.
`PCB_BOARD_PACKER_THREADS=1` selects serial execution. The normal cap is eight
workers or half of available CPUs, whichever is smaller, shared with any opt-in
subtree process workers. A single hypothesis remains serial.

Full ESP32-C3 measurements, separate consecutive Node processes:

| Run | Seconds |
|---|---:|
| Original before changes | 22.755 |
| Parallel 1 | 16.689 |
| Serial control | 26.114 |
| Parallel 2 | 18.465 |

The two comparisons improved wall time by 26.7% and 29.3%. Background host load
was not controlled. No compiler or test suite was running during these four
measurements. Timing includes diagnostic captures, but excludes report rendering.
The original run attributes 21.683 s to 12 block solves, 0.031 s to board packing
and 0.618 s to 13 postrefine calls. The heavy MCU grouped/released pair accounted
for about 20 s of sequential work. The parallel run computes these simultaneously.

The report generator asserts exact final placement equality, equal independent
quality metrics, and equal native solutions including all checkpoints against the
original captured inputs. This is not a fresh full Telemetry performance claim.

`before` uses the original addon. `serial-control` uses an archived addon with
the unsuccessful route-cache experiment disabled (`PCB_BLOCK_ROUTE_CACHE=0`),
and falls back to the serial API. Both parallel runs use the new production addon.
The baseline source revision was `8311f7b`. Binary hashes were not captured for
these measurements, so reproducibility relies on saved problems and exact-output
assertions rather than a retrospectively assigned binary hash.

## Discarded experiments

`esp32c3/after` is an earlier unsuccessful route-score-cache trial, NOT the final
optimized result. Its full run coincided with other work; do not use that time as
an isolated comparison. The six cache replay measurements and four
`benchmark-before-*` / `benchmark-optimized-*` memory-reuse measurements under
`before` did not establish a useful speedup. Neither experiment remains in code.

## Paths and regions

The historical global report explicitly disabled `signalPaths`; it now renders
them and separately shows soft-limit failures. ESPower declares three paths;
its two USB entry segments are 10.26 / 10.61 mm against a soft 7 mm limit.
ESP32-C3 declares two paths and meets their limits. No paths were dropped.

ESPower's saved resolved input, test fixture and current `ESpower.js` declare no
constraint regions. Therefore the reported violation of an ESPower region cannot
be reproduced from this input. The SVG renderer now shows declared regions and
their allow lists. A separate full-placement test verifies that a fixed permitted
antenna stays inside its region while another block remains outside.

## Verification and reproduction

Native release build, TypeScript typecheck and 30 focused tests passed: native
batch equality/order/validation, cache correctness, checkpoint portfolios, paths,
and constraint regions. The normal package build was also run.

```powershell
node --import tsx scripts/experiment-placement-performance.mjs esp32c3 parallel-new
node --import tsx scripts/experiment-placement-performance-report.mjs
```

The report generator reads the four named saved runs and verifies them. To add a
new run to its comparison, explicitly extend its `tags` list. The historical
global regression report remains 14/15 completed pairs; ThunderF722 was stopped
when work shifted to runtime analysis. ESPower here is re-rendered from that
saved result, not newly solved.
