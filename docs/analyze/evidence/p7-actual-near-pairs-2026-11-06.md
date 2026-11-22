# Actual STM32 pair capture and replay

Two ignored library diagnostics in `volume_integral_magnetic.rs` read `P7_SAVED_PROBLEM` and write/read `P7_PAIR_OUTPUT`: `capture_actual_neighbor_pairs` and `replay_actual_neighbor_pairs`. They do not run an electrical solve or change production quadrature. Capture uses the original board mesh builder with a 4 GiB allocation budget, a 120 s internal wall allowance and a 30 s pair-sampling allowance; the actual run additionally used the external supervisor's 120 s / 6 GiB process-tree limits. Inner geometry/quadrature operations may not poll the internal deadline promptly, so external supervision is required.

The first debug capture exceeded the intended internal limit during preparation and was interrupted. No pairs were produced and exact elapsed time was not retained. It is not successful bounded evidence. The subsequent release run used an external supervisor and completed normally.

## Release evidence

Input: `debugging/p7-pair-cache-singleton-600s-20261105/executed-native-request.json`. Output: `debugging/p7-actual-pair-release/comparison.json`, plus `stdout.txt` and `supervision.json` in that directory. Board/geometry digests are `fnv1a64:62b730b266b9e4e8` / `fnv1a64:e3f382312858f66b`. Original copper tetrahedra: 154,899.

External wall time: 84.516 s; sampled private/RSS peaks: 531,111,936 / 479,195,136 B. Of 128 adjacent-index candidates, 33 selected a non-adaptive branch and 95 succeeded as adaptive pairs; no failures. The 16 slowest successful adaptive samples were saved with original indices, coordinates, order 12, tolerance 1e-10 and exact coordinate/result bits. The sample ended at its count limit. Adjacent indices are a local deterministic sample, not the actual full magnetic walk or spatial coverage of the board.

Replay of four saved pairs passes bitwise result comparison in a 0.01 s test. Production adaptive timings are 0.6803–0.7012 ms per pair. A separate fixed-order diagnostic evaluates orders 2/4/8/12 at actual target quadrature points: all 2,312 source evaluations per pair report analytic source order zero, with no positive fallback. Fixed-all-order timings are diagnostic and are not substituted for production adaptive timings. Replay log: `debugging/p7-actual-pair-replay.log`.

## Consequence

These pairs do not reproduce the roughly 43 ms average adaptive callback from the earlier full STM32 prefix. Neither a fallback hotspot nor a kernel improvement is established. Next profiling must cover dispersed and cross-region pairs or capture actual slow walk callbacks. This evidence rules out treating the first adjacent-index sample as representative; it does not rule out expensive fallback elsewhere.

Reproduction: compile with `cargo test --locked --manifest-path native/analyze/Cargo.toml --release --lib capture_actual_neighbor_pairs --no-run`, then run the reported executable with `capture_actual_neighbor_pairs --ignored --nocapture` under the external supervisor and the input/output variables above. Run `replay_actual_neighbor_pairs --ignored --nocapture` against that output under an external 30 s process timeout. Release test compilation and the native addon build pass (`debugging/p7-actual-pair-native-build.log`). The complete extraction time goal and case-matrix acceptance remain open.
