# Positive far-source distance cost and order escalation

This follows [adaptive source orders and real slow callbacks](p7-slow-source-order-2026-11-06.md). The change is confined to the positive far-source summation in `polyhedron.rs`; the global geometric norm, analytic face/edge evaluation, physical domain and acceptance tolerances are unchanged.

For each positive source sample, a finite normal squared distance now uses a direct square root. Overflow, underflow and other non-normal sums retain the original two-hypot norm. Three squares, two additions and the square root introduce only a few floating-point rounding units for a normal sum, covered by the existing 1024-epsilon allowance. This reduces repeated transcendental work without weakening the error bound.

When the first geometrically admissible Gauss order fails its actual mass-error check, the solver tries the next order up to ten. Previously it immediately entered the more expensive positive refinement path even if a higher source order could satisfy the same bound. Each order retains its own deterministic cache.

The dedicated regression injects a rejected mass bound into the order-four cache and verifies that a higher order recovers the integral under the original tolerance. It also exercises a distance whose squared value overflows, checking that the hypot fallback returns a positive finite far potential. The signed-action/full-KVL control and cache eviction controls pass against the final source.

The sixteen saved real STM32 slow pairs are compared against the captured fixed-order-ten values at relative tolerance 1e-10. An initial release check takes 0.034533 s versus 0.052685 s for the preceding adaptive-source implementation; maximum relative difference from the original fixed-order values is 1.7247304746969935e-14. These timings are local controls, not a full-channel speedup.

Logs of final checks: `debugging/p7-distance-guards.log`, `p7-distance-action.log`, `p7-distance-cache.log`, `p7-distance-captured.log`, and `p7-distance-native-build.log`. Full-board time admission and shared-return automatic decomposition remain incomplete. The physical case-matrix is not promoted by these controls.

Final replay of the checked source passes in 0.034676 s with the same maximum relative difference. Guard regression, signed-action/KVL, both bounded-pair controls and release addon build pass. No additional full-board run is used to claim end-to-end acceleration for this incremental change.
