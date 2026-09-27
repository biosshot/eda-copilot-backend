# Early soft alignment in board packaging — Telemetry

Open `comparison.html`. It distinguishes the accepted final board (unchanged)
from an independently reproduced, rejected primary aligned packing hypothesis.

## Implementation

Structural peers are discovered before board packing, without the finishing
pass's 8 mm initial-distance filter. Existing similarity threshold 0.78 and
dominant-footprint/IC/bbox anchor selection are retained. Telemetry has four
eligible pairs: U14/U18, R38/R44, L3/L4, U1/U2.

Rust beam and local-improve scoring receive an optional softAlignment policy:

```
error = clamp(min(abs(dx), abs(dy)) - 0.15, 0, 3)
proximity = clamp(1 - (bboxGap - 8) / 8, 0, 1)
reward = -24 * similarity * proximity * (9 - error²)
```

Unplaced counterparts contribute zero. A bounded reward avoids encouraging
misaligned blocks to separate just to escape a fading positive penalty. Remote
blocks are not pulled together. The same term is included in completed-board
hypothesis and block-portfolio selection. Native/TS parity is tested.

New candidates align transformed footprint centers, including rotated,
asymmetric blocks. They include row/column positions with tight, comfort and
2 mm extra gaps, plus projections of free-rectangle slots onto the target axes.
All ordinary candidates and hard checks remain. This does not introduce atomic
pair placement or neighbourhood repacking; beam candidates remain individual
block placements. Independent evaluations retain native parallelism.

The board packer retains the ordinary primary packing, adds an aligned primary,
and evaluates the tight primary and up to two aligned local-portfolio seeds.
Five hypotheses are available for Telemetry. Final finishing alignment remains.
Mandatory-hint magnitudes cannot worsen during full-board selection, in addition
to the existing native relative-validation check of violation keys.

Board native contract is bumped from 4 to 5 to prevent stale binaries silently
ignoring the new score/candidates. Block contract remains 4. Native and dist were
rebuilt; no live EasyEDA document was changed.

## Comparison and outcome

Baseline: `telemetry-anchored-2026-09-27/after-final.json.gz` (also the unchanged
result of the preceding finishing-only experiment). Same input, local solved
blocks, alternatives and fixed poses. Only board packing and subsequent global
postrefine/finishing alignment rerun. Hashes are in `summary.json`.

Full run took about 272 s and selected hypothesis 0, the ordinary primary. The
accepted final 154 poses match baseline exactly: length 2392.86 mm, 299 straight
line crossings, 296 foreign-pad hits. Ten fixed poses are unchanged. Existing
seven critical 3.2 mm clearance violations remain; absolute validation is false
before/after, relative validation and quantitative hard-hint comparison pass.

To isolate what the new search generated, `probe-summary.json` compares the two
primary hypotheses BEFORE portfolio selection and global postrefine:

| Metric | Ordinary primary | Aligned primary |
|---|---:|---:|
| L3/L4 smaller axis offset, mm | 1.366 | 1.262 |
| U1/U2 axis offset, mm | 0.496 | 0.496 |
| U14/U18 axis offset, mm | 8 | 21 |
| R38/R44 axis offset, mm | 5.707 | 13.207 |
| MST length, mm | 2407.12 | 2435.65 |
| Straight-line crossings | 304 | 309 |
| Foreign-pad hits | 299 | 304 |
| Alignment reward | -324.77 | -330.31 |
| Completed-board selection score | 16198070.57 | 16549028.51 |

The aligned variant changed the arrangement but worsened the final objective and
some mandatory clearances, so it was correctly rejected. It is shown separately
in HTML and NOT exported as the selected ASM. Native rank hardCount=3 for both
is a different diagnostic from the report's individual group-clearance findings.

This is a negative experiment for these settings, not proof early alignment is
ineffective in general. In particular, pairs with axis error >=3.15 mm earn no
reward; the metric does not distinguish the 5.7 and 13.2 mm offsets above. Current
search still places one block at a time, and native packing and completed-board
selection have different objectives. Merely increasing cosmetic weight has not
been tested and is not claimed to solve those limitations.

## Reproduction

From the backend root, with the rebuilt native addon:

```
node --import tsx scripts/experiment-telemetry-repack.mjs after-final after docs/experiments/telemetry-pack-alignment-2026-09-27
node --import tsx scripts/experiment-telemetry-pack-alignment-probe.mjs
node --import tsx scripts/experiment-telemetry-pack-alignment-report.mjs
```

`after.json.gz` and `probe.json.gz` retain snapshots. Uncompressed after.json is
ignored. ASM files contain only movable-component poses and pass
BoardAssembleSchema; they do not alter outline, holes, copper or fixed mechanics.

67 targeted TypeScript tests passed; 52 Rust tests passed and one pre-existing
test is ignored. Tests cover native/TS scoring parity, early pair discovery,
rotated candidates, malformed alignment policies, native deterministic results
on 1/2/4/12 requested threads, fixed poses, obstacles, portfolios and routing
contract compatibility. Typecheck and production build passed. Ordinary/aligned
SVGs were rendered and visually inspected. No routing was run on this Telemetry
comparison.
