# Spatial foil current checkpoint

This is a partial P7 implementation and a reproducible refusal packet.
It does not admit a PCB, via, PDN, complete AC band or transient model.

`fixture.rs` constructs exact finite source geometry and shared spatial
potential traces. The native example uses actual production field/magnetic
primitives and the existing P6 solver. `audit.py` independently checks tensor
C, frozen box self L, smooth box mutual L, rectangular DC and a handwritten
six-state complex reference. Nine diagnostic points pass; eight required
points fail unchanged P6 power/residual/singularity guards. They stay visible.
`test-audit.py` corrupts saved values without replaying native solves.
AC diagnostic losses use peak-phasor `I* R I` normalization (twice cycle mean);
the1A DC loss is actual steady watts.

`scenario.js` is the direct existing DSL. `dsl.ts` only injects the saved native
candidate and runs that script through the disposable process/compiled worker.
It verifies whole-band refusal and that an intentionally failed `a.check`
does not stop execution. No SavedScenario or second electrical runtime exists.

From repository root, with `CARGO_BUILD_JOBS=1`:

```powershell
cargo test --manifest-path native/analyze/Cargo.toml --release --test current_space
cargo build --manifest-path native/analyze/Cargo.toml --release --example current_space
python tests/fixtures/analysis-local-extraction/current-space/run.py
python tests/fixtures/analysis-local-extraction/current-space/audit.py
python tests/fixtures/analysis-local-extraction/current-space/test-audit.py
python tests/fixtures/analysis-local-extraction/current-space/plot.py
node --import tsx tests/fixtures/analysis-local-extraction/current-space/dsl.ts
```

`run.py` refuses to overwrite evidence and enforces120s/512MiB externally.
Its output includes code/binary/compiler hashes and before/after hashes of all
eight captured boards. Static figures require visual inspection; interactive
browser controls are not claimed. See the full original-obligation/readiness
matrix in `docs/experimental/analyze/p7-completion-verification.md`.
