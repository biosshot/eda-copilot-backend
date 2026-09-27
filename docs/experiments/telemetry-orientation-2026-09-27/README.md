# Telemetry — soft agreement of block orientation

Open comparison.html. It separates the unchanged accepted final board from
generated packing hypotheses, including the visually useful but rejected #3.

## Change

Alignment center and orientation reference are separate. The center still uses
the dominant footprint/IC/bbox policy. Direction uses a main IC with the largest
pin count, with footprint area and reference as deterministic tie breakers. If
there is no main IC, the dominant alignment component is used when available.

Telemetry pairs:

| Position references | Direction references |
|---|---|
| R38/R44 | U16/U20 |
| L3/L4 | U13/U17 |
| U1/U2 | U1/U2 |
| U14/U18 | U14/U18 |

Numbered pad centroids establish the library-frame offset. Quarter-turn matches
permit modest positive x/y scale factors (0.8–1.25) for library land-pattern
spacing differences; reflections and pin renumbering are not accepted. Residual
error must be <= max(0.05 mm, 3% pad-pattern radius). Incompatible patterns get
no orientation preference. Bottom-side mirroring reverses the relative offset.
This compares physical footprint direction, not functional pin equivalence.

U1/U2 have different library zero angles (U1-to-U2 offset 270 degrees). They are
already physically aligned in the baseline, despite different numeric rotations.
U13/U17 and U16/U20 differ by 90 degrees in the baseline.

The added reward is bounded and soft:

```
-120 * similarity * proximity * (1 + cos(correctedRelativeAngle)) / 2
```

It uses the existing distance falloff (bbox gap 8–16 mm). Zero/90/180 degree
differences earn full/half/zero reward. All allowed native rotation candidates
continue to be evaluated; no orientation is forced. The same term participates
in completed-board and block-portfolio selection. The finishing alignment pass
still performs only translations; the existing global postrefiner retains its
own objective. The report exposes angles before and after those stages.

Board native contract is 6; block contract remains 4. Native and dist rebuilt.

## Experiment

Baseline is the preceding accepted pack-alignment result, which equals the
earlier anchored/spacing baseline. Same Telemetry input and saved local block
portfolios; local block solving is not repeated. Five packing hypotheses plus
portfolio selection, global postrefine and finishing alignment took ~250 s.
All hypotheses are captured in after.json.gz. Capture uses the native solve
cache, so it does not rerun each native search.

Orientation error order below is U14/U18, U16/U20, U13/U17, U1/U2; all values
are corrected for library orientation and measured before global postrefine.

| Hypothesis | Angles, degrees | MST mm | Line / foreign-pad crossings |
|---|---|---:|---:|
| 0 ordinary | 0 / 90 / 90 / 0 | 2407.12 | 304 / 299 |
| 1 aligned + direction | 0 / 90 / 90 / 0 | 2419.84 | 306 / 300 |
| 2 tight | 90 / 0 / 90 / 0 | 2248.02 | 257 / 241 |
| 3 local alternative 1 + alignment/direction | 180 / 0 / 0 / 0 | 2196.17 | 247 / 259 |
| 4 local alternative 2 + alignment/direction | 0 / 90 / 0 / 0 | 2242.98 | 285 / 282 |

Hypothesis 3 aligns both requested IC pairs and improves wiring proxies, but
worsens five mandatory clearances versus ordinary packing. Examples: hv_neg to
adc 0.45 -> 0.03 mm; hv_pos to usb_charge 1.96 -> 0.52 mm, both requiring 3.2 mm.
It also reverses U14 relative to U18. This variant includes different local
block layouts, so its improvement cannot be attributed to orientation alone.

The selection guard rejects hypotheses 2–4 on mandatory-hint magnitudes, and
hypothesis 1 on score. Accepted result is still ordinary hypothesis 0 followed
by the same refinements; every final pose matches the baseline. Final metrics
remain 2392.86 mm, 299 line crossings, 296 foreign-pad hits. The seven existing
critical clearance violations remain: absolute native validation is false,
relative change validation and hard-hint non-regression pass. No routing or
live EasyEDA mutation was performed. ASM exports contain the accepted result,
not rejected hypothesis 3, and pass BoardAssembleSchema.

## Reproduction and validation

```
node --import tsx scripts/experiment-telemetry-repack.mjs after-final after docs/experiments/telemetry-orientation-2026-09-27
node --import tsx scripts/experiment-telemetry-pack-alignment-report.mjs docs/experiments/telemetry-orientation-2026-09-27 docs/experiments/telemetry-pack-alignment-2026-09-27/after.json.gz
```

69 distinct targeted TypeScript tests passed, including native/TS orientation
parity, library-angle normalization, mirrored-pattern rejection, bottom-side
offset, fixed poses and deterministic native parallel search. Rust: 52 passed,
one existing ignored test. Typecheck and build passed. Generated hypotheses 0/3
were rendered and visually inspected; final before/after poses coincide.
