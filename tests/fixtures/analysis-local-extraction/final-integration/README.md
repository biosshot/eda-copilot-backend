# Bounded final-integration controls

These controls do not close P7. They check automatic current Krylov congruence
in an already accepted finite-foil space, all unit-port drives, independent
opposing-drive references, current/local-loss recovery, the existing P6 worker
interchange, explicit source-piece contacts and finite void ownership.

The full board extraction builder, ordinary signal/return vias, scalable
potential reduction and intermediate-frequency diffusion losses are absent.
The scripts never write to historical evidence or `native/analyze/debugging/`.

From the repository root, `CARGO_BUILD_JOBS=1`:

```powershell
cargo test --release --manifest-path native/analyze/Cargo.toml --test automatic_reduction --test finite_contacts
cargo build --release --manifest-path native/analyze/Cargo.toml --example automatic_reduction
python tests/fixtures/analysis-local-extraction/final-integration/run.py
python tests/fixtures/analysis-local-extraction/final-integration/audit.py
python tests/fixtures/analysis-local-extraction/final-integration/test-audit.py
python tests/fixtures/analysis-local-extraction/final-integration/plot.py
node --import tsx tests/fixtures/analysis-local-extraction/final-integration/dsl.ts
```

`run.py` refuses to overwrite its packet and supervises the numerical executable
at 120 s / 512 MiB. Compilation is separate. The time-labelled repeat reconstructs
the model and reduction; no immutable-geometry cache speedup is inferred.

For a fresh checked version, pass a new output directory to `run.py`, `plot.py`,
`test-audit.py` and `dsl.ts`. `audit.py` accepts the packet path followed by its
output directory. The final guard check uses
`debugging/p7-final-integration/checked/`; earlier packets remain unchanged.
