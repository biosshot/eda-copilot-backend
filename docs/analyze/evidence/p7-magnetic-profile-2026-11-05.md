# P7: intermediate adaptive magnetic profile, 2026-11-05

Stage C of the [acceleration plan](../plans/p7-acceleration-plan.md). Diagnostic instrumentation only; traversal, arithmetic order, tolerances, certificates and numerical outputs are unchanged. No full-board benchmark is authorized in this batch.

## Interface and interpretation

Set `P7_MAGNETIC_TRACE=1` in the native process environment and capture stderr. The default emits no profile records. Lines prefixed `P7_MAGNETIC_TRACE ` contain JSON with a process-local scope ID, thread ID, phase, optional axis, status and elapsed wall seconds. RHS column scopes occur in input order within a batch. Keep raw logs; they are not acceptance certificates.

Phases: batch, geometry cache preparation, common plan construction, cover walk, RHS column, axis, source/upward preparation, apply walk, downward translation and leaf recovery. A scope writes start and complete records. Error unwinding writes unfinished; external process termination cannot write a final record, so use the last streamed checkpoint. Walk progress is sampled every 4096 visited node pairs and emitted at most once per second per walk. A long single callback may delay the next checkpoint; this is not an asynchronous watchdog.

Walk records include visited, far, near, planned exact-pair and zero counts, plus accumulated classification, far-callback and near-callback seconds. Counts describe classified blocks, not completed callbacks or necessarily executed pair kernels: a checkpoint before a callback includes that block, and near callbacks on axes 1/2 skip the kernel because axis 0 already processed all three components. Completed classification/callback timers exclude the currently executing operation. Scope times nest: do not add batch, plan, column, axis and walk totals together. Callback time in cover construction measures bookkeeping, not field application. Profiling adds per-visit clocks and streamed logging when enabled; diagnostic timings are not clean production-speed measurements.

The profiler retains fixed-size counters and does not store pair lists or duplicate current buffers. It does not measure allocator peaks; process-tree memory remains the supervisor's responsibility. Existing certificate evidence retains the chosen-order histogram and contraction estimates after successful completion.

## Repeated-work assessment

| Work | Code evidence | Candidate / limitation |
|---|---|---|
| Cover decisions | Common envelope includes every column and axis; `Plan::new` walks it once, then `apply` walks it per axis and column | Replaying immutable decisions could remove repeated classification. Measure counts and storage before retaining a cover; an unbounded pair cache is not acceptable. |
| Separation and geometric Taylor coefficients | `decide` computes separation/tail conditions; far application calls `jet_required` for each axis/column | Geometric coefficients for a fixed block/order can be reused or generated once for a batch. Per-block storage may be large; no cache policy chosen yet. |
| Temporary vectors | Far application creates powers, moments and local contribution vectors | Reuse bounded scratch after measuring allocation cost; current profile groups this cost into far callback time. |
| Source envelope per axis, moments, translations and contraction | Values depend on the actual current vector | Recompute values; do not reuse results across different RHS merely because the geometry is identical. |
| Near kernels | Axis 0 computes all three components; later axes skip near pair work | Existing optimization must be preserved. Across RHS, identical kernel coefficients may offer batching, subject to bounded storage. |

For two nonzero RHSs the code can replay the common decisions six times after their initial construction. This is a count of redundant traversals, not a sixfold runtime claim. The relative cost of classification versus far/near callbacks on STM32 is still unknown. First decision after profiling: determine whether cover replay, geometric coefficient batching or scratch reuse addresses the measured dominant cost. CPU parallelism remains later work.

## Verification

The numerical control uses three tetrahedra and existing signed-action/KVL reference comparisons; it is not a full STM32 field solve or a physical case-matrix acceptance.

- `cargo check --locked --manifest-path native/analyze/Cargo.toml --lib`: passed (13.23 s; existing warnings).
- `npm run native:build`: passed; release addon rebuilt. Build log: `debugging/p7-magnetic-profile-native-build.log`. This is compilation, not a numerical full-board run.
- Focused `cargo test --locked --manifest-path native/analyze/Cargo.toml --lib hierarchical_signed_original_actions_and_full_kvl_enclosures_match_bounded_reference -- --nocapture`: one passed with profile enabled, and one passed with profile disabled. Enabled numerical test runtime was 6.01 s in debug; build time is separate.
- Parsed 400 JSON records, 200 scopes; every start has a complete/unfinished counterpart. Two unfinished scopes correspond to the existing deliberate refusal control. Disabled run emits no profile lines. Numerical evidence matches exactly after removal of timing fields.
- One completed cover walk visits 9 node pairs; 36 completed apply walks visit 324 in total across the tiny test's controls. Aggregate apply classification: 0.0020593 s; far callbacks: 0.0264816 s; near callbacks: 0.0756909 s. These are instrumented debug measurements on the tiny control, not scaling estimates or clean speedup benchmarks. Here classification is much cheaper than callback work; the observation does not rank STM32 bottlenecks.
- Local logs: `debugging/p7-magnetic-profile-test.log`, `p7-magnetic-profile-disabled-test.log`, `p7-magnetic-profile-summary.json`. The small control exercises terminal/error records, not the 4096-visit progress cadence or external termination behavior. No large run was used to exercise those paths.

Next measurement must separate far geometric coefficient generation from current-dependent contractions and allocation if far callbacks dominate a representative bounded run. Until then, retain cover replay, batched geometric coefficients, bounded scratch reuse and cross-RHS near batching as candidates; no production optimization or memory-heavy pair cache is selected by this batch.
