# P7 shared-return finite-domain EM control

This continues the [section lift and grouped cover work](p7-section-return-batches-2026-11-05.md). It validates a small coupled field model, not production routing, open-domain physical accuracy or complete case-matrix coverage.

## Implemented model and admission

Actual 1 kHz section recovery is lifted into two contiguous extrusion sections with two signals and the same physical finite return conductor. An explicit physical source-to-section-conductor map rejects wrong/missing ownership. Original endpoint contacts and individual potential coordinates at the internal cut are retained. All three physical component constant modes remain distinct. One `R^-1 L` current enrichment uses the common physical magnetic operator; no full-solution snapshots train the candidate. Physical R/L/C are projected once; section impedance/capacitance stamps are not added.

The reusable candidate solve uses the existing conditioned charge-coordinate system and checks component charge balance plus original-space KCL/KVL. Invalid frequency/injection dimensions/nonfinite inputs and insufficient section-only candidates are rejected.

Ordinary reduced solve had up to 3.6% individual-port discrepancy despite tiny 1 A-normalized KCL/KVL. Its island imaginary charge errors reached about 2.21e-20 C, against charge magnitudes near 6.33e-19 C. Conditioned charge coordinates reduced these errors near 1e-36 C and resolved the discrepancy. This is a weak-charge numerical conditioning issue; no accuracy threshold was relaxed. The general default coupled solve was not changed; only the existing method's crate visibility was extended for explicit candidate use.

## Controls

Two independent balanced drives inject +1 A at a signal input, -1 A at its output, +1 A at return output and -1 A at return input. All four physical leg-to-return port voltages and both loop drops are compared with the full finite-domain volume solution. Both sections carry the required -1 A return current. The internal cut is moved from 1.0 to 0.7 mm at fixed 2 mm total length; the full reference is recomputed on each corresponding mesh.

| Basis | Potential/current coordinates |
|---|---:|
| Full physical | 24 / 216 |
| Section-only | 6 / 11 |
| Distributed trace | 24 / 29 |
| Trace + one magnetic enrichment | 24 / 47 |

The enriched candidate passes for both drives and cuts: maximum relative individual-port difference below 8e-16, KCL below 4.5e-16 A, KVL below 7.6e-21 V. Section-only admission is a negative control. Current coordinates are reduced; this small mesh retains all potential coordinates after adding the cut. No runtime acceleration or voltage-space reduction is claimed.

Focused control: `cargo test --locked --manifest-path native/analyze/Cargo.toml --lib section_return -- --nocapture`, passed in 0.97 s. Log: `debugging/p7-shared-return-em-control.log`. Compact certificate regression also passes: absent optional grouping fields remain absent instead of becoming JSON null and triggering erroneous grouped validation.

Release addon build passed; log `debugging/p7-shared-return-em-native-build.log`. No full-board run is represented as verification of the new shared-return candidate.

## STM32 grouped-cover measurement

The existing release build was run with `P7_MAGNETIC_BATCH_COLUMNS=1`, unchanged physical board/ports, 12 GiB native / 14 GiB process-tree cap, 410 s native / 420 s supervisor and one diagnostic enrichment iteration. Artifact: `debugging/p7-singleton-cover-420s-20261105/`.

The **first column** cover completed in 38.148 s: 39,743,535 visits, 3,531,639 far, 15,604,494 near and 739,382 zero blocks. Previous common cover across columns had 394,309,166 visits and 150,044,919 near blocks at 297.503 s. These are different cover scopes: do not claim a whole-batch speedup from this ratio. Later singleton covers were not reached.

The run ended at native deadline after 413.657 s outer wall. Sampled peak private/RSS: 1,875,525,632 / 1,066,102,784 B. The first application reached 92,632 near callbacks; recorded kernel times were 182.797 s adaptive (4,105 calls) and 82.711 s separated (88,527 calls), including the final failed call. No complete magnetic action or electrical response; <=600-second full extraction remains unachieved.

## Remaining case-matrix obligations

### Extended longitudinal and remote-bridge controls

Residual-driven enrichment now solves physical DC correction problems from the reduced model's KCL residual. It does not use a full-solution training snapshot. Global zero-contact potential modes preserve each physical island constant by compensating the anchor basis column; their gradients enrich the current space. Inputs, retained-coordinate order and aggregate allocation plans are checked. Each call permits at most five residual rounds and three magnetic rounds.

The original two-cell control remains. With four longitudinal mesh cells over the same 2 mm line, only the internal section cut is retained. The full 60/432 potential/current coordinates reduce to 36/107. Both drives and cut positions pass unchanged charge/KCL/KVL admission; maximum relative complex-port difference is below 6.3e-11. These are axial mesh cells, not PCB stack layers.

A separate physical fixture has two signals, two return strips and a copper bridge only in the remote final slab. Removing the bridge leaves the source identifiers intact but replaces its geometry with dielectric. At three and five slabs, the connected geometry has three conducting components and the broken geometry four; DC transfer closes only with the bridge present.

For the three-slab bridge, a 1 A AC transfer between the return strips at 1 kHz explicitly requires the remote connection. The candidate after three residual/magnetic rounds is rejected at KCL 3.1701e-7 A. This negative control is retained. Two additional residual-only rounds pass the original charge/KCL/KVL gates, reducing full 76/468 coordinates to 38/145. Relative complex-voltage differences at the two tested ports are 1.31e-14 and 1.34e-13 against the full finite-domain model. No gate was relaxed. Broken-bridge AC and the five-slab AC case are not admitted by this evidence.

Verification: all three `section_return` library tests pass in 13.56 s (`debugging/p7-long-return-control.log`). This remains a private candidate and a finite-domain equivalence control, not an automatic production route or a measured STM32 speedup.

Both `volume_projection` integration tests also pass (`debugging/p7-long-return-projection.log`); the release addon build passes (`debugging/p7-long-return-native-build.log`).

Remote vias and disconnected-return AC, floating uncontacted islands, slots and multiple reference planes, layer/width/via events, arbitrary orientations and mesh mappings, frequency validation and automatic planner integration remain unverified for this new candidate. Existing production paths remain in place. The 42-case acceptance inventory is not promoted. Next implementation must address these through real geometry and original-space admission, not inferred net connections or ROI clipping.
