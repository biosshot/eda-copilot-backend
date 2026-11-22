# Reusable full-source island response

`island_response::IslandResponse` extracts the previously test-only compensated
constant-potential correction into a private reusable module. It retains one
full-source Field response per physical conductive island and the island-level
Jacobian. Storage scales with island count times original potential count,
plus island count squared; no source island or coupling is dropped. Retained,
scratch, live Space and Field allocations are checked before construction.
Partition validation requires every original potential exactly once.

`refine` retains compensated island constants and composed nodal charges through
the unchanged 1e-8 component-relative charge gate. It accepts real and imaginary
injections, allows at most eight correction steps, checks solve residuals and
refuses nonfinite state, stagnation, singular required solves or resource limits.
Full-pivot LU applies no regularization. Finite extreme inputs cannot exploit an
overflowed residual scale to pass a relative gate. Deadlines are checked during
construction, field actions, composition and refinement; field actions also need
their own bounded implementation, as in the existing Field.

The factor is lazy: an already conserved state needs no nonsingular Jacobian.
Controls require zero Field/zero injection to pass without correction and the
same singular Field/nonzero injection to refuse. Complex injection signs and a
multi-node island's aggregate charge are checked separately, with no per-node
charge zeroing. Invalid/duplicate/incomplete partitions, deadlines, allocation
limits and finite-input scale overflow refuse explicitly.

The real three-island Field diagnostic now calls this module, while independently
checking unchanged original magnetic KVL, KCL and compensated port corrections.
Both drives at 1/10 kHz pass; the deliberate 1 microvolt perturbation is retained.
It compares against rounded direct Field action explicitly rather than claiming
that rounded evaluation passes the small island-charge gate.

The final external 120 s / 2048 MiB release supervisor passes 34 controls in
9.969 s (`debugging/p7-axial-retry-island-final-controls/`). Sampled peak
private/RSS is 20,701,184 / 34,275,328 B (20 ms lower bounds). Real Field drives
require at most one correction, with maximum relative port change 3.1931e-16.
The native addon build passes in
`debugging/p7-axial-retry-island-native-build.log` (`BUILD_EXIT=0`).

This module is not connected to automatic candidate admission. Its composed
state, port corrections and device bridges need an explicit shared-return state
contract before dispatch. It does not admit the q60 mixed magnetic Gram or extend
section bandwidth. Per-mode section retention is unchanged; all case-matrix
acceptance and the complete extraction target remain open.
