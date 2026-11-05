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

Remote physical bridges/vias and their disconnection, floating uncontacted islands, slots and multiple reference planes, layer/width/via events, arbitrary orientations and mesh mappings, more longitudinal cells per section, frequency validation and automatic planner integration remain unverified for this new candidate. Existing production paths remain in place. The 42-case acceptance inventory is not promoted. Next implementation must address these through real geometry and original-space admission, not inferred net connections or ROI clipping.
