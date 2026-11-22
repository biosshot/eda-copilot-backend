# P7: partitioned covers and section-to-return mapping

This implementation continues acceleration directions 1 and 2. It does not complete the common-return architecture or the 42-case physical acceptance matrix.

## Magnetic interaction work

`P7_MAGNETIC_BATCH_COLUMNS=1..64` selects sequential groups of current columns; absent/0 retains the existing common cover. Geometry is reused. Original per-column goals, interval checks and original-coordinate KVL checks remain unchanged. Each group has its own source envelope; certificates retain group goals/bounds and aggregate provenance, validated on the TS boundary. Cache identity includes the explicit grouping policy.

The bounded 16-tetrahedron thin-foil control compares common, pairs and singleton groups. For localized currents, weighted near-pair applications are **124 / 64 / 34**, weighted M2L contractions **76824 / 46620 / 30123**, while summed cover visits increase **90 / 106 / 142**. For overlapping currents, near work stays 124 and cover visits increase **90 / 180 / 360**. Therefore grouping is an opt-in candidate, not an unconditional optimization. Common near blocks split into 16 self and 15 exhausted-tail-order cases, with zero overlapping-ball cases in this fixture; this does not establish the cause distribution on STM32. Evidence: `debugging/p7-near-partition-control.json`.

The signed-action control checks chunked actions against singleton results and independent reference enclosures under unchanged goals, together with existing KVL gates. It passes in 6.44 s. Six cache/validation tests pass, including distinct policy cache keys, valid grouped evidence and tampered bounds rejection; typecheck passes. No new STM32 speedup is claimed.

## Common-return foundation

`section_return::lift` uses actual 1 kHz 2D section current recovery and conforming extrusion geometry to produce real/imaginary physical current columns. Two sections share the same physical return cells; basis overlap is allowed while copper geometry is not duplicated. `Basis::include_current_profiles` adds these profiles through the physical resistance metric without changing harmonic potential coordinates or island constants. It constructs a candidate and does not admit it as an extracted model.

The compact control verifies integrated slab current for two signals and a shared return, preserves three physical component coordinate sets, and rejects nonconforming geometry, missing support and malformed profiles. Mapping test passes; existing volume-projection regressions pass 2/2.

**Still missing:** distributed interface potential/current trace closure; physical conductor/source correspondence beyond the conforming mapping; original-coordinate full/reduced EM comparison using shared R/L/C; remote bridge and broken bridge EM controls; cut relocation and automatic planner integration. P1 section profiles are projected to P0 volume currents: integrated current is controlled, but P1 loss preservation and full EM accuracy are not established. No section impedance/capacitance stamp is added, avoiding duplicate field energy.

## Case-matrix protections

The existing [42-case protection map](p7-coverage-acceleration-map-2026-11-05.md) remains mandatory. No production region selection or routing changed. Remote return vias, slots, finite edges, independent planes/islands, component bridges, differential/group/branch channels, via spans and layer variants retain their existing paths and limitations. The new mapping is not selected automatically for any case. Tests above are mechanism controls, not evidence that all cases are accepted. No numerical accuracy gate or geometry was relaxed.

Final release addon build passed (`debugging/p7-partition-section-native-build.log`). Coverage inventory remains pending; no acceptance statuses were raised. Full-board measurements are not part of this bounded implementation batch.
