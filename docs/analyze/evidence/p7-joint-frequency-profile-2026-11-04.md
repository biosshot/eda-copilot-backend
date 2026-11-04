# P7 W6 profiled joint frequency extraction, 2026-11-04

The native `skin-volume-v3` policy accepts exact joint sample requests from 1,000 through 100,000 Hz. The existing automatic DSL derives the requested band and sends higher demand to a closed physical joint candidate before segmented checks. Package limits still constrain the common request and leave explicit coverage gaps. No DSL syntax was added.

Optional `profileUpperHz` freezes the copper mesh and physical ownership through the base and supplemental requests. Supplemental comparison refuses a mesh or ownership mismatch. The default permits one additional batch with at most min(8, floor(64 / ports)) points, subject to the shared deadline. Audit points do not extend the replay network grid. The strict sensitivity gate is 2%. A result above 2% through 3.5% requires review and remains unverified. Cache lookup revalidates the operator policy and effective upper frequency metadata.

| Focused check | Result |
| --- | --- |
| Low-band audit, planner and automatic path | 15 passed in 2.127 s |
| Cache | 3 passed in 0.990 s |
| Automatic, group and high-frequency routing after integration | 12 passed in 3.358 s |

Typecheck and package build passed. After the AC plating guard, the final package build passed (JS 161 ms, DTS 10.966 s). Two focused adapter tests passed in 0.960 s: original STM32 request transport and plating eligibility/band rejection. Layer and plating AC materials now share the finite 1–100 kHz applicability check; DC behavior is unchanged.

Native test compilation passed in 7m49s after two compile-only corrections. The addon build passed in 9m55s; SHA-256 `6973701143486d92c8c7d06edc60d7d94d1e742df5a38b8aa5bf5e3d1aedc554`.

## Supervised thick-copper control

The geometry check passed in 0.094 s wall time (0.05 s reported by the test). The single 100 kHz thick-copper check passed in 12.406 s wall time (12.37 s reported by the test). The supervisor recorded 12.656 s total, 22,716,416 B peak private memory and 37,502,976 B peak RSS. The supervisor limit was 1,830 s and 4,096 MiB with one thread and 1 GiB host reserve. The test requested 1,800 s and 3,072 MiB. The native P7_VERIFY defaults clamped the actual limits to 300 s and 2,048 MiB, as recorded in the report. Both controls completed within those limits, and the physical solve was run once.

At 100 kHz the model retained 60 copper tetrahedra, with 36 volume tetrahedra and eight prisms in the mixed representation. The full basis had 36 potential and 180 current coordinates. Seven enrichment steps yielded 19 potential and 30 current coordinates. Planned allocation was 52,724,763 B. Skin depth was 208.98 µm and the 100 µm bottom copper used two slabs. Full KCL residual was 7.6205844e-9, full KVL residual was 3.8703133e-10, and power discrepancy was 1.8045461e-15 W.

Artifacts are under `debugging/p7-joint-skin-profile-20261104/native/` in `comparison.json`, `supervision.json` and `foil_reduced.txt`. Independent physical accuracy remains unverified.

The automatic path can return conditional exact samples up to 100 kHz. The physically accepted bands remain unchanged. Skin transition, high-frequency accuracy, physical common-band extraction and all open case-matrix acceptance remain pending.
