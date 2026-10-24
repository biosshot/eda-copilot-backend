# Frozen magnetic-kernel stability and complete physical-space coupons

CPU/F64 internal primitive tests only. Full derivations, failures, uncertainty,
matrices, spectra and cost limits are in
`docs/experimental/analyze/magnetic-kernel-verification.md`.
No Python/external solver runtime dependency is introduced.

`stm-668-556.json` preserves the authored source supports. `coupon.json` adds
four asymmetric finite bars and declares integral0.2%, energy5%, raw reciprocity
1e-9, absolute near-zero entry`.002sqrt(LiiLjj)` and explicit physical space.
`reference.json` contains independent exact-double-z4D and box convolution/6D
references. `slender-self-reference.json` separately encloses the finite-width
source668 self integral; its insufficient earlier QMC uncertainty is retained.
`negative-evidence.json` preserves original range failures and first-candidate
quadrature refusals/under-resolution. Accepted C snapshots are untouched.

`closed-coupon.json` freezes12 exact finite prisms and two continuous P1
streamfunction hats. All internal normal current jumps and exterior normal
flux vanish, with Jz=0; every combination is a finite closed physical current.
`closed-reference.json` stores all78 scalar-pair reference values/scrambles and
full2×2 matrices. `closed-verification.json` includes every144 directed native
pair/time, full error spectrum, eigenvalues and **raw** reciprocity. Order8/12
FAIL remains. `closed-control-preflight.json` fixes the one order24 control;
it passes1e-9 without symmetrizing raw coefficients. Reference uncertainty is
explicitly empirical/statistical; no universal continuum certificate is claimed.

Ordinary focused Rust tests require only tracked JSON:
`cargo test --manifest-path native/analyze/Cargo.toml --target-dir native/analyze/target --test magnetic_kernel --test local_extraction -j1 -- --test-threads=1`.
Four magnetic and eleven existing local tests cover source accuracy, both
directions, transformations, source/geometry/order negatives, complete partial
and closed operators, normal flux and uniform quadratic-energy bounds.

Offline evidence commands, only in a coordinated heavy slot:

1. `python tests/fixtures/analysis-local-extraction/magnetic-stability/native-probe.py build`, then explicit`legacy`/`current` modes. Actual Rust sources/pinned cached rlibs; old source snapshots can be recovered read-only with`git show afe8ba3:...`.
2. `native-probe.py reference` and`slender-self-reference.py` only if intentionally regenerating an absent reference packet; saved green references should be reused.
3. `closed-coupon.py prepare`, then`build`,`native`,`reference` as separate explicit modes. Geometry/flux/pair-count/256MiB preflight is fixed before solve. Maximum process512MiB/60s.
4. `closed-coupon.py control` runs only the predeclared24 rule, once; never an automatic tuning series.
5. `audit.py`,`audit-closed.py`,`plot.py`,`plot-closed.py` recompute saved-value audits/figures only.

New requests refuse repeated recorded attempts. Original snapshots, binaries,
raw output, resource logs, source/input/output manifests and viewed PNGs remain
under ignored`debugging/p7-design-checkpoint/magnetic-stability/`.
Offline generators/references use installed NumPy/SciPy/mpmath/psutil/Matplotlib;
Rust runtime/CI does not. Dense ROI cost remains blocked; no large ROI, public
DSL, real-board, via, C/P6/Thermal API or placement/GPU work is admitted here.
