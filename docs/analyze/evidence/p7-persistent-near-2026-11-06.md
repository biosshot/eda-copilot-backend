# Persistent near-kernel caches and observable progress

The prior [four-worker STM32 attempt](p7-parallel-near-2026-11-06.md) timed out
without parallel-compatible kernel counts. This change retains each worker's
64-cell geometry/cubature cache across windows and divides each ordered window
into indexed chunks of at most eight jobs. Each Rayon worker locks its own cache;
there is no common lock around quadrature. Results still replay in original
order, preserving the existing geometry, gates, interval bounds and certificates.

## Memory and reporting

Admission reserves 66 × 192 KiB cache/scratch slots, a 2 MiB stack and 4096 bytes
metadata per worker, plus 8 KiB window storage. Four workers reserve 60,317,696 B.
This is an explicit memory-for-preparation trade; the default remains serial.
Diagnostics/scalar-cache serial fallback remains unchanged.

`P7_MAGNETIC_PARALLEL_PROGRESS` reports successful kernel windows/pairs, kernel
window wall time, self/separated/adaptive call counts and accumulated per-call
elapsed time, plus geometry cache hits/build attempts. Counts include failed
kernel attempts. Successful window counts do not prove a completed action or
channel; a caller deadline may still interrupt replay. Every report is explicitly
partial. Failure is sticky across window reports and operator destruction.
Reports occur at roughly five-second window boundaries, on window failure and
on operator destruction; a single long window cannot report until it returns.
Per-call elapsed sums are not OS CPU-time measurements.

## Controls

The external 120-second, 2048 MiB tree supervisor passed fifteen controls in
9.937 s, with sampled private memory 18,685,952 B and RSS 31,703,040 B (20 ms
sampled lower bounds). Logs: `debugging/p7-persistent-near-final-controls/`.
These include bitwise 1/2/4-worker kernel/action/bound/certificate checks,
window counters and deadline failure, the original signed/full-KVL enclosure,
eleven hybrid controls, and two cold/warm captured windows per worker policy.

The captured benchmark now uses a 64-slot serial cache as well as 64 slots per
parallel worker. The supervised timing run overlapped release compilation, so
its times are not a controlled performance comparison. Earlier focused timings
also showed no established incremental gain beyond the prior parallel path.
Cache retention and finer scheduling therefore remain experimental, without a
new full-board acceleration claim. One four-worker replay reported 256 adaptive
calls, 128 geometry builds and 384 hits across two windows; results matched bits.

The native addon build passed; its local log is
`debugging/p7-persistent-near-native-build.log` (release 7m11s, `BUILD_EXIT=0`).

## Instrumented STM32 attempt

The cold four-worker attempt ended at the native magnetic-action deadline after
243.203 s external wall, before the 250-second supervisor cap. Native budget was
240 s/12 GiB, tree cap 14 GiB, maxIterations=4, RHS chunks one, global Rayon one;
trace/scalar cache/capture and the other experimental geometry flags were off.
The source board and RS485_B port definitions remain unchanged. All copper,
islands and barrels are retained. No channel result was completed.

Final counters (operator-lifetime partial data, not a completed action):

| Quantity | Measured value |
|---|---:|
| Successful kernel windows / pairs | 1,051 / 134,528 |
| Separated kernel calls / elapsed sum | 125,647 / 186.050367 s |
| Adaptive kernel calls / elapsed sum | 8,885 / 78.525765 s |
| Self calls | 0 |
| Kernel-window wall time | 83.710641 s |
| Geometry builds / cache hits | 128,556 / 140,508 |
| Sampled peak private / RSS | 1,885,736,960 / 1,095,852,032 B |

Call totals include entered failed kernels; successful-window pairs exclude the
failed trailing window. Per-call elapsed sums overlap across workers and are
not whole-program CPU time. Their sum divided by window wall is approximately
3.16, indicating useful concurrency inside these windows. Separated kernels
account for approximately 70.3% of the summed kernel elapsed time. Roughly
159.5 s of total wall lies outside kernel windows (preparation, traversal,
far work and orchestration are not separately timed here).

The final progress record retains `failed=true`; no completed-action count is
available. Local evidence: `debugging/p7-persistent-near-stm32-250s/`, especially
`stdout.txt`, `comparison.json`, `supervision.json` and `identity.json`. Native
core identity: `13ae8c7b75ff07aa3ac5262dffd54e20d4af5708cbee65ecd7039f19e0a2d7cc`.

This is not a paired full-board acceleration measurement against the previous
600-second run. The full <=600-second target and case-matrix acceptance remain
unmet. The measured next priority is reducing separated-kernel arithmetic:
the current order-8 direct tensor evaluates 8^6 point pairs. For exactly
vertical-edge tetrahedra, integrating both axial coordinates could reduce the
outer work to four dimensions without merging cells or assuming uniform current
across a prism. This requires numerical/error/fallback validation before use.
