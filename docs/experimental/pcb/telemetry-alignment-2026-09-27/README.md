# Telemetry: conservative soft alignment

Open `comparison.html` for the full board and three paired crops. The delivered
before/after placements are identical: the pass found no admissible improvement.
This is an intentionally retained negative result, not an alignment success.

## Baseline and reproduction

Baseline is `telemetry-anchored-2026-09-27/after-final.json.gz`, including anchored
connector assembly, spacing and global postrefine (prior branch state fa0194a).
Input is `global-placement-2026-09-27/Telemetry/input.json`. SHA-256 digests are in
`summary.json`. Run from the backend root:

```
node --import tsx scripts/experiment-telemetry-alignment.mjs
```

Only the new finishing stage is evaluated against the saved complete board; block
assembly and board packing are not rerun. Production synchronous and asynchronous
auto-place both invoke this same stage after global postrefine, preserving its
input as stage 03-v2-post-place and recording 03b-v2-board-alignment separately.

## Policy

- Similarity >= 0.78, bbox gap <= 8 mm, same layer. Weighted component family
  histogram (IC pin count included), typed local net-edge histogram, and dominant
  footprint dimensions. Reference numbers, pin ordering and net names do not
  matter. This is a structural approximation, not graph isomorphism or proof of
  electrical function.
- Anchor: largest footprint if its area is >= 1.5 times the runner-up; otherwise
  a main IC with >= 70% of the largest area; otherwise bbox center.
- Frozen initial pair set; bounded soft cost 24 * similarity * min(error,3)^2,
  where error is the smaller difference between x/y anchor centers.
- Only rigid translations: full/half/quarter correction on either axis, moving
  either block or both; perpendicular offsets 0, +/-0.5, +/-1 mm. No rotations,
  internal reconstruction or forced proximity. Two passes, <= 3 mm cumulative
  displacement per component from the original board.
- Candidate must improve both alignment and the combined existing board score,
  soft spacing and alignment cost. The original result remains the fallback.
- Wiring score excludes hint penalties for its separate admission guard: total
  degradation <= min(40 score units, 0.1% baseline score). Each scored net's MST
  may grow by at most max(0.5 mm, 2% baseline). Budgets are cumulative from the
  initial layout, not reset after each move.
- Geometry/native change checks and a quantitative mandatory-hint check apply.
  The latter is necessary: native relative validation only compares violation
  keys, allowing an existing violation's magnitude to increase.

## Result

L3/L4 families score 0.954 despite the extra diode. U1/U2 score 0.922, but their
bbox gap is 9.29 mm, outside this deliberately local 8 mm neighbourhood. Other
eligible pairs are the low-charge families and isolated supplies. Their selected
anchors are R38/R44 (largest footprints) and U14/U18 respectively.

74 candidates evaluated, zero accepted: 56 fail objective/alignment improvement,
2 electrical score, 1 individual net length, 15 mandatory hint magnitudes. These
are first-failure counts, not independent counts of every defect. The isolated
supply axes differ by 8 mm, beyond the maximum combined 6 mm correction, so they
produce no proposals.

A preliminary L4-family x shift of -0.622 mm would halve the axis offset, but
would reduce existing required-clearance gaps to logic_power and usb_charge.
It was rejected after adding the magnitude guard and is NOT in the final output.

Length 2392.86 mm, 299 straight-line crossings, 296 foreign-pad hits remain
unchanged. Seven pre-existing critical 3.2 mm clearance violations remain; full
absolute native validation is false both before and after. Relative change
validation and the ordinary geometry report pass. No routing was performed.

ASM exports contain only movable component poses; fixed mechanics, board,
holes and copper are excluded. Both exports pass BoardAssembleSchema. They have
not been applied in EasyEDA and do not fix the baseline clearance violations.

## Validation

57 targeted tests passed across board alignment, block portfolio, island
placement and native postrefine. Includes a positive free-pair alignment case,
fixed pose preservation, keepout rejection, similarity with an extra diode,
renamed nets, deterministic output, and the Telemetry mandatory-gap guard.
Typecheck and production build passed. The generated board SVGs were rendered
and visually inspected; before and after coincide as expected.
