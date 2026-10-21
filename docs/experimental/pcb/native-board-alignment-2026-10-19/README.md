# Native board alignment pass, 2026-10-19

Question: does moving the board alignment finishing pass (`refineBoardAlignment`) into Rust, as one call like `refinePostPlacement`, keep its results exactly, and what does it save end to end? Follows [issue #19](https://github.com/biosshot/eda-copilot-backend/issues/19), option B.

Host: Apple M5 Max, 18 logical cores, macOS. Node 22.22.3, rustc 1.99.1, default (GPU-enabled) build; the post-place and board stages ran on CPU (`no compatible F32 Vulkan GPU`). Native SHA-256 prefixes: `main` (`3bc1a08`) `0ff49588991e2f04`, this branch (`9aa2521`) `d8b7040c5d6764e3`.

## Design

- TypeScript finds the similar block pairs (unchanged) and compiles the pass once: each component's geometry at its own rotation and layer with x = y = 0, the score template of `encodeNativePostPlaceScoreProblem` with targets instead of coordinates, the spacing pairs, the report checks and mandatory-hint rules with the baseline violations, and the refine problem used by `validatePlacementChange`.
- Rust generates the candidates and runs every stage in the original order: objective (post-place score, spacing, alignment), electrical regression, placement-report geometry, mandatory hints, native change validation.
- Exactness. The TypeScript pass computed geometry on JS numbers (`sourceCoordinateAdd`, `coordinateDifference`, `Math.round`, `toFixed(3)` keys) and each addon call localized its own frame from the coordinates it received. The native pass reproduces that arithmetic and computes each score in the frame its standalone call would choose.
- Incremental. A candidate updates only the components it moves. The route plan was 99 % of a candidate's score (16.7 of 16.8 ms on Portable). `RouteCache` keeps, for every alternative path of every link, its nonzero obstacle and crossing terms in summation order, and a candidate recomputes only the terms of moved pads and of links of their nets. Zero terms do not change an F32 sum and the bounding-box tests only skip terms that cannot be nonzero, so the total is bit-identical to `nets_plan`, which the cache checks when it is built and keeps as fallback.
- Parallel. Each pair's candidates are independent checks against the layout at the start of the pair. Workers take them in order with private scratch worlds and use the objective limit of the resolved prefix, which is never tighter than the serial one; acceptance then applies the exact serial limits in candidate order. Threads follow `PCB_POST_PLACE_THREADS` and its fallbacks; the synchronous API uses one.

## Parity

`tests/pcb-board-alignment-native.test.ts` compares the full result (placements, pairs, errors, moves, diagnostics, `evaluated`, every rejection counter) with the previous TypeScript pass, kept as `tests/helpers/board-alignment-reference.ts`, with 1 and 3–4 native threads:

| Case | Candidates | Accepted | Rejections exercised |
|---|---:|---:|---|
| Free pair | 117 | 1 | objective |
| Keepout at the target axis | 30 | 1 | objective, geometry |
| Mandatory distance already exceeded | 15 | 0 | hardHint |
| Hard block anchor gap | 15 | 0 | native |
| STM32G0_CAN_RS485 capture | 797 | 3 | objective, electrical, geometry |
| Historical Telemetry layout | 278 | 0 | objective, electrical, netLength, geometry |

The alignment inputs of the 15 benchmark fixtures were also captured from `main` (`refinementInput`, tree roots, refined placements and the TypeScript result) and replayed through the native pass with 1 and 16 threads: all 15 results are identical to the captured ones.

## Alignment pass alone

TypeScript times come from the capture run on `main` (2 / 16 / 16), native 1-thread times from the replay (Rust `totalMs`), 16-thread times from the end-to-end runs below.

| Fixture | Pairs | Candidates | Accepted | TypeScript | Rust, 1 thread | Rust, 16 threads |
|---|---:|---:|---:|---:|---:|---:|
| Portable_AC_DC_Telemetry | 6 | 1130 | 0 | 80.7 s | 2.81 s | 0.33 s |
| STM32G0_CAN_RS485 | 2 | 797 | 3 | 9.0 s | 0.26 s | 0.06 s |
| STEPPER_CONTROLLER | 1 | 317 | 0 | 8.4 s | 0.17 s | 0.03 s |
| ThunderF722 | 1 | 208 | 0 | 7.1 s | 0.41 s | 0.07 s |
| H743 | 1 | 0 | 0 | 0.2 s | — | 0.03 s |

The other ten fixtures have no alignment pair.

## End to end, 15 fixtures

`PCB_LAYOUT_SUBTREE_WORKERS=2 PCB_BOARD_PACKER_THREADS=16 PCB_POST_PLACE_THREADS=16`, configurations interleaved (order reversed on the second pass), median of 2 passes, hash of the final poses. Measured with no other fixture or build running.

| Fixture | main | this branch | Change | Poses |
|---|---:|---:|---:|---|
| H743 | 73.6 s | 75.5 s | +2.6 % | identical |
| ThunderF722 | 71.3 s | 64.8 s | −9.1 % | identical |
| ICM20948_v2 | 56.3 s | 56.8 s | +1.0 % | identical |
| Portable_AC_DC_Telemetry | 185.9 s | 112.1 s | **−39.7 %** | identical |
| STEPPER_CONTROLLER | 40.6 s | 32.9 s | −19.0 % | identical |
| rp2040_base | 23.1 s | 22.6 s | −2.4 % | identical |
| STM32G0_CAN_RS485 | 28.8 s | 20.2 s | −29.9 % | identical |
| ESpower | 16.1 s | 16.1 s | −0.4 % | identical |
| ELRS | 12.8 s | 12.3 s | −3.5 % | identical |
| dc-dc | 5.8 s | 5.8 s | −1.6 % | identical |
| TPS63060DSCR | 4.7 s | 4.6 s | −3.1 % | identical |
| esp32c3 | 3.8 s | 3.9 s | +2.4 % | identical |
| BAND_V3_SIGNAL_PATH | 1.1 s | 1.1 s | +3.3 % | identical |
| CONNECTOR_OUTWARD | 0.7 s | 0.7 s | −11.0 % | identical |
| CONNECTOR_ROTATIONS | 0.6 s | 0.6 s | 0.0 % | identical |
| **Total** | **525.3 s** | **429.9 s** | **−18.2 %** | identical |

Portable_AC_DC_Telemetry, STEPPER_CONTROLLER and esp32c3 exit with rc=1 from their own checks, on both builds. Boards without alignment candidates differ by −3.5 % to +3.3 %; H743 is +2.6 % in both passes, of which the alignment pass is 27 ms.

A first run of this comparison showed board packing 6–13 % slower on the branch: sharing the spacing and route helpers had added allocations to paths that board packing, block solving and post-placement refinement also use. Commit `9aa2521` restores those paths; the table above is measured after it. For comparison, the discarded process-pool prototype (`perf/board-alignment`, 16 workers) brought Portable from 200.2 s to 132.3 s.

## Verdict

The native pass returns the same placements, moves and counters as the TypeScript pass on every case measured, and removes the alignment pass from the critical path (Portable 80.7 s → 0.33 s). Over the 15 fixtures the total drops 18.2 %, with identical poses. Not measured: the GPU backend for board packing and post-placement, which this pass does not use, and hosts other than this one.
