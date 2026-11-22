# P7: bounded acceleration implementation attempts, 2026-11-05

Target: complete extraction in at most 600 seconds, with every required case-matrix family preserved. This is an implementation/measurement ledger, not P7 physical acceptance. GPU and via 2.5D remain excluded. The user permits explicitly marked, measured modest accuracy tradeoffs; no numerical tolerances or physical inputs have been relaxed in the changes recorded here.

## Baseline and diagnostic repair

Commit `b2bd533` records route closure diagnostics and magnetic tracing. The initially chosen `P7_MAGNETIC_PROFILE=1` collided with the pre-existing production numerical-profile selector (`baseline` / `compact-v1`): the first full runner refused before numerical work. The diagnostic switch and record prefix are now `P7_MAGNETIC_TRACE`. This corrects the earlier standalone-only verification gap; the old switch must not be used for tracing.

The corrected cold full-run baseline used 170 seconds native / 180 seconds outer deadline, 3072 MiB native / 4096 MiB process-tree private cap and 1024 MiB host reserve. It stopped after 86.547 seconds with `LOCAL_MEMORY_LIMIT`, before completing an electrical result. Sampled process-tree private/RSS peaks were 785,485,824 / 684,732,416 B. Artifacts: `debugging/p7-acceleration-full-baseline-valid-20261105/`. The first failed launch is retained separately in `debugging/p7-acceleration-full-baseline-20261105/`.

Memory admission location: mixed native work plus retained map/electric plan supplies 2,762,683,331 B; exact interaction cache adds 23,521,056 B; magnetic hierarchy then needs further storage while only 2,827,022,194 B total native budget remains after earlier reservations. Actual sampled memory before a refused allocation is not evidence that its planned peak can be ignored. Added stage-specific diagnostics expose these terms. A possible electric-preparation peak-versus-retained-lifetime overestimate remains unproven; no memory guard was reduced.

## Implemented candidates and verification

- Far-block scratch reuse: four temporary vectors are allocated/reused per axis instead of per far interaction. No retained pair cache; interval accumulation and traversal order preserved. Tiny signed/reference action and KVL control passed in debug (5.87 s numerical test). Its full numerical evidence exactly matches the prior baseline after timing fields are removed. This establishes correctness on the control, not a full-board speedup.
- Capability dispatch: replaces a fixed 10 kHz decision with actual producer block/component bands and geometry admission. Production section bands are not widened. High-band inputs may still select joint because current producers are narrow-band. Requested interval gaps remain explicit. This is a routing architecture improvement, not proof of broad-band hybrid accuracy.
- Runner accepts explicit finite time/native-memory/tree-cap declarations to support bounded experiments; original board, materials, port pair and source-hash provenance remain intact.
- Local cache tests: 11/11 pass with workspace-local `TEMP`/`TMP`. Default sandbox temporary paths caused four disk-reopen failures (`ENOENT`), so no unrelated cache implementation change was made. New trace setting does not alter numerical model/cache identity.
- Geometry/routing regressions: 24/24 in 4.4 s, covering rotations, bends, coupled sections, split/wide/remote returns, via, meander, physical ownership and STM32 closure diagnostics. Log: `debugging/p7-acceleration-geometry-tests.log`. These are not 42 physical acceptance results.
- Coverage validator remains `ready=0 pending=42`; inventory and physical acceptance are distinct. No case or reference was removed to obtain a passing result.

## Shared-return localization assessment

The current independent section/local interfaces do not expose a shared distributed plane trace with compatible current/flux transfer and nonduplicating energy ownership. The old resistor-graph prototype supplies DC topology only; production partitioned conduction reduces the conductive correction while retaining global field work. Existing controls therefore cannot validate deleting remote copper or replacing the plane by independent local grounds. No arbitrary ROI truncation was implemented. Stage B's minimum two-section common-plane EM equivalence test and a usable localized production model remain outstanding; this is a substantive architecture failure to meet the acceleration target, not a completed feature.

## Subsequent bounded measurements

- Release addon build after scratch changes: passed (5m12s compilation). Capability dispatch commit `7830569`: 19 focused tests and typecheck passed. Scratch/admission diagnostics commit `288d556`.
- A 6144 MiB native request was rejected by the existing `BOARD_FOIL_BUDGET` maximum of 4096 MiB before numerical work (3.375 s outer run); recorded under `debugging/p7-acceleration-full-600s-20261105/`. The runner was corrected to respect that maximum; no production cap was bypassed.
- Full default-iteration attempt with 590 s native / 600 s outer, 4096 MiB native / 6144 MiB tree cap stopped after 85.375 s with memory refusal; private/RSS peaks 826,806,272 / 689,369,088 B. `debugging/p7-acceleration-full-600s-valid-20261105/`. The logged base was map 1,630,412,416 + electric plan 426,076,899 + native work 706,194,016 B, against remaining budget 3,900,764,018 B.
- Commit `5197e39` fixes simultaneous reservation of sequential mixed/full work, replaces discarded mixed-map assembly scratch with owned retained capacities (keeping the original conservative full-space bound), and distinguishes voltage/current mode bounds. Constant floating voltage modes remain; only nonexistent current storage is removed. Two focused native memory tests pass; independent code/lifetime review found no physical omission. Release addon build passed (5m06s).
- Next full-path diagnostic explicitly capped enrichment at **one iteration**, leaving all residual/power gates unchanged. This is a resource-limited diagnostic, not evidence that the default 16-iteration request fits. With the same 600 s / 6144 MiB outer limits it reached mixed harmonic preparation and refused after **85.453 s** at harmonic free residual **4.25567357155269e-8**, above the unchanged 1e-8 gate. Private/RSS peaks: 1,005,613,056 / 746,381,312 B. New map reserve: 1,001,710,582 B; harmonic admission: 3,600,983,489 B < 3,900,764,018 B available. Artifact: `debugging/p7-acceleration-memory-fixed-600s-20261105/`. Thus the memory fix measurably passed a previously blocking preparation stage, but no AC result or magnetic action completed.

## CPU parallelism assessment

Adaptive RHS, axis and walk loops are serial; changing `RAYON_NUM_THREADS` alone does not parallelize them. The geometry and near-pair caches use `RefCell`, so sharing them across RHS workers is not safe without redesign. Separate worker state would multiply large moments/local/tail arrays; a shared locked near cache could serialize work. No unmeasured threading patch or fictitious 1/2/4 speedup is claimed. Stage D remains unimplemented pending a bounded shared-cache/worker-memory design and representative measurements.

## Harmonic preparation refinement

The next candidate replaces a single DC residual correction with at most eight corrections using the same admitted factor. Corrections accumulate with the existing compensated arithmetic, but acceptance is evaluated on the actual stored F64 voltage. Retained electrodes are not updated; the original deadline is checked each iteration; the residual limit remains 1e-8. Failure includes the residual history rather than hiding stagnation.

`cargo test --locked --manifest-path native/analyze/Cargo.toml --lib foil_reduced:: -- --nocapture`: **5 passed, 1 ignored** (the ignored full STM32 benchmark was not launched). Includes the new high-contrast analytic series-network control, retained-contact bit preservation, normalized loss and deadline refusal, plus existing shared conductive trace and memory controls. Log: `debugging/p7-harmonic-refinement-tests.log`.

Release addon build passed (5m01s), committed as `da2fed6`. The next bounded full-path attempt (`debugging/p7-acceleration-refined-600s-20261105/`) passed harmonic preparation after two corrections: residual **5.452796962685071e-9**. It then refused partitioned factor memory admission at **84.641 s**, private/RSS peaks **1,008,242,688 / 751,177,728 B**. This fixes the measured residual failure without relaxing accuracy, but does not complete extraction.

## Partitioned preparation memory correction

Commit `7d273f2` removes only the global triplet/CSC assembly reservation when `correction` selects the partitioned backend and returns before constructing those global buffers. Owner assembly, trace assembly, factors, cells, vectors, modes and descriptors retain their guards. The omitted unused reservation is **475,849,728 B** for this fixture. Independent review found no omission on the active path; the unpartitioned path is unchanged. Six focused native tests pass (one heavy test ignored); release addon build passes (5m17s). Logs: `debugging/p7-partition-memory-tests.log`, `debugging/p7-partition-memory-native-build.log`.

The final cold full-path diagnostic (`debugging/p7-acceleration-partition-fixed-600s-20261105/`) retained all **844 copper regions, 65 islands and 89 barrels**, with the original physical ports. Limits: 590 s native / 600 s supervisor; 4096 MiB native / 6144 MiB process-tree cap; one enrichment iteration, as explicitly recorded in the request. It refused with `LOCAL_MEMORY_LIMIT` after **88.437 s**. Sampled private/RSS peaks: **1,010,159,616 / 757,673,984 B** (20 ms sampling lower bounds). Full-mode admission improved to **3,170,360,257 B**; partitioned factor initial admission passed at **3,365,500,737 B** against **3,900,764,018 B**. The first owner factor completed (23,112 unknowns); a subsequent preparation allocation was refused. The generic error does not identify that final guard precisely. Actual memory observed before allocation refusal does not justify disabling conservative guards.

## Final disposition of this implementation attempt

Independent code/log review further localizes the final refusal to second-owner assembly admission (an inference, not a directly labelled checkpoint): of 464,697 current coordinates across two owners, the second must own at least 290,078, reserving at least 594,079,744 B of assembly scratch. Initial live storage plus the first retained factor brings the next phase to at least 3,973,190,449 B, exceeding the 3,900,764,018 B budget by at least 72,426,431 B even before additional trace/coupling allowances. Reducing that reservation requires a justified assembly/lifetime change, not disabling the guard.

| Plan stage | Delivered | Not achieved / failed candidate |
|---|---|---|
| A: region selection | Reproducible closure trace, physical ID/count invariance, matrix-family inventory | Smaller physically equivalent automatic region; conservative closure still captures the whole fixture |
| B: common return | Architecture audit distinguishes DC prototype, conductive Schur and distributed EM return | Distributed return interfaces, energy ownership, two-section EM equivalence control and production localization remain unimplemented |
| C: magnetic work | Opt-in streaming trace, bounded scratch reuse with unchanged control results; preparation memory and residual fixes | Full STM32 adaptive action profile and measured full-board speedup: preparation still refuses first |
| D: CPU parallelism | Identified serial loops and unsafe-to-share RefCell caches; assessed per-worker memory cost | No parallel implementation or 1/2/4-worker speedup claim |
| E: frequency routing | Fixed executor threshold replaced by producer capabilities and actual block bands; 19 focused tests/typecheck pass | Current producer band still ends at 10 kHz; wide-band hybrid models were not implemented or certified |
| F: acceptance | Bounded cold runs, 24 geometry/routing and 11 cache controls, focused native controls and release builds | No complete electrical result, no warm-result comparison, no <=600 s success, no full 42-case physical acceptance |

**The <=600-second extraction target is unmet.** All repeated full-path runs above have explicit terminal outcomes; none is an unfinished background benchmark. No accuracy threshold was relaxed and no remote return, island or case was removed. The one-iteration diagnostic cap can prevent convergence and is not a production-default performance result. Coverage validation remains `ready=0 pending=42`; passing regressions cannot be presented as proof that every physical case is ready.

The highest-value remaining work is a bounded distributed common-return model and physically justified region closure, with a minimal full-domain equivalence control before localization. Further memory work needs stage-specific owner/trace lifetime measurements; magnetic parallelism needs a shared immutable geometry/cache design and admitted worker scratch. Merely raising budgets or adding threads has not been established as a solution. GPU and via 2.5D were not introduced.
