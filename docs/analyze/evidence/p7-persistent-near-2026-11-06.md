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
`debugging/p7-persistent-near-native-build.log`. An instrumented bounded board
attempt is still pending. Do not promote case-matrix acceptance or claim the
<=600-second channel target from these controls.
