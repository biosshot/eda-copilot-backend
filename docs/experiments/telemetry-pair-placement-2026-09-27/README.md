# Telemetry: additional pair placement candidates

The accepted board is unchanged. The new proposal was rejected; this experiment does not establish a layout-quality improvement on Telemetry.

## One search policy and a fallback

- Pack the same primary block interiors with comfort spacing. Preserve this ordinary complete result.
- Add axis candidates and atomic pair transitions to the same beam search, using a non-negative alignment penalty. No tight-board or alternative-portfolio whole-board runs.
- Compare the proposal with the ordinary result, retaining geometry, fixed-placement, quantitative mandatory-hint and electrical guards.
- Preserve the existing neighbourhood portfolio selection after packing (12 blocks, 249 candidates, 8 accepted), then global postrefine and the small final alignment pass. Removing that established stage initially changed the final board; it was restored before recording this result.
- Local block alternatives are still generated and available to the neighbourhood selector. The two complete packing attempts use identical primary block interiors.

Position error is `max(0, min(abs(dx), abs(dy)) - 0.15)`, with cost `8 * similarity * huber(error)`; Huber is quadratic through 1 mm and linear beyond it. Orientation cost is `24 * similarity * (1 - cos(angle)) / 2`, using the numbered-pad footprint-frame correction. No reward or candidate-distance attenuation is present. The final small translation pass uses the same penalty shape and weights, on its existing nearby-pair set.

The ordinary cheap shortlist keeps up to 32 candidates; axis candidates receive up to 16 additional slots. Pair transitions are supplemental: ordinary one-block transitions remain. Structural similarity must be >= 0.85 for joint placement (>= 0.78 for the alignment score). For the strongest remaining peer, try anchor positions, allowed rotations, four sides and multiple gaps. Joint candidates preserve ownership and are filtered against actual geometry. States at different placement depths are not ranked against each other. Search and local candidate evaluation retain native parallelism.

The electrical acceptance guard permits at most min(40 score units, 0.1%) wiring-score increase, and checks every scored net's MST length against baseline + max(0.5 mm, 2%). These are conservative experiment tolerances, not routing validation or evidence of a global optimum.

## Observations

- Telemetry only; unchanged input and saved local portfolios. Input/native hashes are in `summary.json`.
- Ordinary packing exactly reproduced the previous ordinary packing's component poses.
- The aligned beam generated 388 retained atomic pair candidates, on 6 native lanes.
- Raw proposal: R38/R44 axis error 5.707 -> 4.207 mm; U1/U2 unchanged at 0.496 mm; L3/L4 unchanged at 1.366 mm. Orientation errors unchanged.
- Raw MST 2407.12 -> 2419.84 mm; line intersections 304 -> 306; foreign-pad hits 299 -> 300.
- Mandatory clearance hv_pos -> usb_charge worsened 1.96 -> 0.75 mm; hv_neg -> logic_power worsened 0.49 -> 0.42 mm (both require 3.2 mm). Total completed-board score and independent wiring score also worsened.
- Proposal rejected. Final 154 poses, including 10 fixed poses, exactly match the previously accepted board. Final MST 2392.86 mm, 299 line intersections, 296 foreign-pad hits.
- Relative native validation and quantitative mandatory-hint guard pass. Absolute native validation remains false for pre-existing constraints; this work does not fix those violations.
- Full offline run: 113.82 seconds. No routing or live EasyEDA mutation.

The corresponding high-voltage inductor pair is L3/L4 (hv_pos/hv_neg); L2 belongs to logic_power. Position anchors for the charge pair are R38/R44; orientation anchors are U16/U20.

## Reproduce

```powershell
npm run native:build
node --import tsx scripts/experiment-telemetry-repack.mjs after-final after docs/experiments/telemetry-pair-placement-2026-09-27
node --import tsx scripts/experiment-telemetry-pair-report.mjs
```

`comparison.html` shows accepted before/after boards and crops. A collapsed section separately shows the rejected packing proposal and its failed clearances. Both ASM files contain the accepted poses; no rejected placement is labelled as accepted or exported for application.

Validation: 71 TypeScript tests passed across board alignment, native packing/parallel determinism, block portfolios, anchored spacing, island placement and route comparison. Rust: 55 passed, one existing ignored. Typecheck and build passed. New tests cover supplemental single/pair transitions, ordinary-shortlist retention, keepouts, non-saturating axis penalty and individual-net regression rejection. Board native contract is 7; block contract remains 4.
