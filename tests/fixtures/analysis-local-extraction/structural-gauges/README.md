# Structural gauge follow-up

Run from repository root with CARGO_BUILD_JOBS=1. New output is exclusively
`debugging/p6-p7-structural-gauges/`; the runner refuses to replace coupon.json.
Historical inputs and independent analytic/box coefficients are reused from
`debugging/p7-completion/parent-coupon.json` and `parent-audit/references.json`.
No expensive continuum or board corpus reference is rerun.

```
cargo build --release --manifest-path native/analyze/Cargo.toml --example current_space
python tests/fixtures/analysis-local-extraction/current-space/run.py
python tests/fixtures/analysis-local-extraction/structural-gauges/audit.py
python tests/fixtures/analysis-local-extraction/structural-gauges/test-audit.py
python tests/fixtures/analysis-local-extraction/structural-gauges/plot.py
node --import tsx tests/fixtures/analysis-local-extraction/current-space/dsl.ts
```

The current direct DSL expectation admits the bounded insulated coupon. Its old
refusal scenario and failed frequency packet remain unchanged in historical
p7-completion evidence. This does not admit arbitrary PCB/via extraction.
