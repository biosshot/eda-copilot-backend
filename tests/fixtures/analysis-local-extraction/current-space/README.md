# Spatial foil current checkpoint

The source-bound foil geometry/trace/current fixture remains a bounded internal
P7 coupon. The structural-gauge follow-up now checks its complete original
17-point 1 kHz..100 kHz axis. It does not admit arbitrary PCB/via extraction or
close P7. Historical failures, independent references and old refusal scenario
remain unchanged under `debugging/p7-completion/`.

`fixture.rs` contains the unchanged geometry and frozen original axis. The native
example calls production field/magnetic primitives and P6, retaining full raw C.
`run.py` writes only new `debugging/p6-p7-structural-gauges/` output, refuses to
replace existing coupon evidence and supervises 120 s / 512 MiB numerics.
Code/binary/compiler, historical evidence and all eight board hashes are saved.

`scenario.js` is direct existing DSL. `dsl.ts` injects saved extracted full and
reduced physical two-port models and uses the real disposable process/compiled
worker/shared addon. It plots admitted longitudinal responses and verifies
continuation after an intentionally false `a.check`.

See [the follow-up runner instructions](../structural-gauges/README.md) for the
current independent audit and static plots. The old `audit.py`, `test-audit.py`
and `plot.py` in this directory intentionally remain historical refusal-audit
utilities and operate on the old p7-completion packet.

AC losses use peak-phasor `I* R I` normalization (twice cycle mean); DC loss is
actual steady watts. Static images must be visually inspected; interactive
browser controls are not claimed.
