# Internal layered capacitance coupons

Only planar meshes,69-or-fewer(or three-foil93) z planes and explicit finite
metal/material/source definitions are stored; no volume matrices/fields.
`generate-frozen.py` is an offline reproducible generator, not a native/CI
runtime dependency. `expected.json` freezes accepted Python matrices AND the
failed D32 case. Original raw unsymmetrized FastCap reference remains alongside.

Predeclared gates: exact-mesh full quadratic parity1e-10; independent full
energy error5%; separate domain/mesh2%; free/charge/work/recovery1e-9.
Analytic dielectric/affine patch target1e-10 relative energy. Different native
CDT triangulation is checked against independent reference anew. Shifted/three
finite free-space references must come from offline FastCap, not from this kernel.
Insulated 1D dielectric coupons have a separate explicit natural boundary and
analytic full-C reference; they do not stand for infinity-truncated vacuum.
No public analyzer, real-board, via or L admission follows from these tests.

`native-evidence-summary.json` preserves compact native full matrices, plans,
resource measurements, source hashes, all failed controls and separate changes.
`references/` contains raw shifted/three FastCap2 matrices, primary solver source
commit/binary hash, stdout/stderr and input-generator hashes. Reference n16→32
full changes are0.15484%/0.17802%; these are observed changes, not certified bounds.
Three-foil raw4.90099% passes5%, but adding its observed reference change gives
5.07901%: no uncertainty-inclusive5% acceptance is claimed. Three-foil mesh
control adds only8 preselected longest shared interior edges/plane in the
.58..82mm high-flux annulus to stay within50k; it is not a global error bound.

Reproduce small gates from repository root:
`cargo test --manifest-path native/analyze/Cargo.toml --test layered_capacitance --test local_extraction -j1 -- --test-threads=1`
and `cargo test --manifest-path native/analyze/Cargo.toml --lib independent_affine_constant_and_asymmetric_epsilon_overlap_patch -j1 -- --nocapture`.
Heavy cargo tests are explicitly ignored by default. No addon/public interface
was added. The existing pinned faer dependencies are reused by the offline
optimized source harness, without changing crate profiles or GPU defaults:

1. `python tests/fixtures/analysis-local-extraction/layered-native/build-native-evidence.py`
2. `python tests/fixtures/analysis-local-extraction/layered-native/run-native-evidence.py accepted-d64-mesh` (or another named JSON; optional`--cdt`).
3. `python tests/fixtures/analysis-local-extraction/layered-native/audit-native-evidence.py`
4. `python tests/fixtures/analysis-local-extraction/layered-native/plot-native-evidence.py`

Each native request is externally supervised at512MiB/60s, and retains original
internal deadline through assembly/symbolic/factor/recovery. Opaque faer calls
are checked before/after, so external supervision supplies hard interruption.
Raw outputs remain ignored under`debugging/p7-design-checkpoint/layered-native-runs/`.
Only offline tools require Python/NumPy/psutil/Matplotlib; Rust runtime/CI does not.
`generate-frozen.py` sorts the complete z set before plane association; an initial
unsorted-extension fixture was rejected before assembly and preserved in debug.

## Reference margin follow-up (§8.17)

The summary above remains the original §8.16 snapshot. A single higher offline
three-foil n48 reference now completes the architect's working screen:
4.880861% native full error +2×0.034255% observed last reference change =
4.949371%≤5%; reference changes16→32=.178017%,32→48=.034255% strictly decrease.
This is an explicit working screen, not a certified continuum bound. Raw source,
domain/mesh and algebraic uncertainties remain separately recorded.
Native matrix/source/binary are unchanged and revalidated; no native control or
new native tests/solves were needed. Production/native512MiB/60s is unchanged.
Only this one offline reference used authorized1024MiB/60s; actual790548480bytes/
39.82094s. Preflight selected n48 using saved panel/resource scaling; the wall
estimate34.81s was optimistic, while the externally enforced60s cap passed.

`references/three-48-{preflight,run}.json`, `references/three-48/` raw matrix/logs,
and `references/three-margin-summary.json` preserve source hashes, all three
references, every signed raw-entry error, full positive-range errors, scalar
maxima and screen outcomes. Earlier n32 screens/failures are not overwritten.
Offline `refine-three-reference.py` prepares preflight only unless`--run` is
explicit; it refuses a second recorded attempt. `audit-reference-margin.py`
recomputes from saved matrices; `plot-reference-margin.py` renders saved values
to ignored`debugging/p7-design-checkpoint/three-reference-margin/audit.png`.
