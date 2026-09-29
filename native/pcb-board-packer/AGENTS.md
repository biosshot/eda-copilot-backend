# Native PCB solver

`src/block_solver.rs` assembles blocks; `src/solver.rs` packs the board; `src/post_place_refine.rs` refines completed placements; `src/micro_router.rs` estimates route cost; `src/model.rs` defines native data. Before adding a scorer, cache, or geometry utility, search these modules and the TypeScript encoders in `src/pcb-layout/pcb-auto-place-v2/native/` for the existing representation. Keep contract versions and both sides of each changed field synchronized.

Preserve deterministic candidate reduction and hard-constraint checks. Profile expensive paths on saved exact inputs before optimizing, and compare quality plus wall time afterward. Use the existing bounded Rust parallelism; account for process/subtree workers so threads do not multiply beyond the CPU budget. The addon is isolated in process workers, not multiple Node worker threads in one process.

After Rust changes, run `cargo test --manifest-path native/pcb-board-packer/Cargo.toml` and `npm run native:build` from the repository root, then focused TypeScript tests and native replay for the affected solver. Do not infer full-board speed from a single warm replay.
