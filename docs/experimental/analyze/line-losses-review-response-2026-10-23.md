# P7b review response: R1/R2 investigation

Status: R1 criterion decision approved by the reviewer on 2026-10-23; the reviewer closed the R2 equivalent
criterion on 2026-10-23. The unchanged 5% numerical
target now applies to the two fixed R1 cases; original Qucs discrepancies remain
visible engineering comparisons. The executable Rust physical-reference tests remain ordinary tests. There is
no new public solver/API, and the coordinated diagnostic-only addon rebuild and targeted smoke checks pass.
The only production change so far is the requested skin-depth reason text.

## R1: independent physical reference for the same fixed geometries

`tests/fixtures/analysis-line-losses/boundary-panel-reference.py` is a standalone
**test-reference generator**, independent of Rust/FEM. It imports NumPy/SciPy,
not production code. Its exact inputs are the two original air rectangles,
unchanged sigma=58e6 S/m and frequencies 20/5 GHz. The ground is the same
infinite thick PEC reference used by the surface-impedance perturbation model.
There is no fitted multiplier, rounding, corner clipping or geometry change.

The half-space Dirichlet Green function is
`G=(1/(2π epsilon0)) log(|r-r_image|/|r-r_source|)`.
Piecewise constant charge panels on all four rectangle faces use analytic
logarithmic segment integrals, collocated at panel midpoints. Solve the dense
boundary integral equation for the uniform conductor potential and normalize
by its total charge to obtain the 1 A current distribution. Signal Gram is the
exact constant-panel integral of its square. Ground current is obtained from
the image Poisson kernel, not an FEM return reaction. Its integral over the
entire infinite reference equals -1 A. For source points `(a,y)` and `(b,z)`,
`integral P_y(x-a) P_z(x-b) dx = P_(y+z)(a-b)`, so the complete ground Gram
uses a closed infinite-domain convolution and eight-point panel quadrature.
Thus the reference has no finite outer boundary or reference-current cutoff.
The usual `R=Rs B`, `Z=sqrt(mu0/epsilon0)/(C0/epsilon0)` and
`alpha_c=R/(2Z)` recover peak-phasor power and units explicitly.

Reference validation does not use the production solution:

- An exact circular cylinder over the ground checks C, signal and ground Gram
  separately. With radius .1 mm, center height .5 mm and 512 panels, relative
  errors are -6.976e-6, +5.755e-6 and -6.534e-7 respectively. They decrease
  quadratically under refinement. The analytic expressions are
  `C=2πepsilon0/acosh(H/a)`, `Bsignal=H/(2πa sqrt(H²-a²))`,
  `Bground=1/(2π sqrt(H²-a²))`.
- A square far from the ground checks sharp-corner loss against Wheeler's
  polygon rule, equivalent to the inscribed circle. The 640-per-face result
  differs by -0.47285%; the observed convergence is consistent with the
  rectangular-corner field `r^(-1/3)` and cosine panel grading. No corner values
  are clipped. The remaining-corner estimate is retained separately from raw
  values; it is not substituted into production.

The fixed-reference raw histories retain 160/320/640/1280 horizontal-face
panels (short faces use comparable geometric resolution). Finest totals are
2890 and 2816 boundary panels. Linear equation residuals are <2e-15. Estimated
reference uncertainty is conservatively 1%, based on refinement and the known
corner asymptotic, **not a rigorous error bound**.

| Original case | Qucs alpha | Native alpha | Independent finest alpha | Corner-limit estimate | Native vs finest | Native vs estimated limit |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| w=.3,h=.18,t=.005 mm;20 GHz | .7994527375 | .7007235918 | .7244195650 | .7280658967 | -3.27103% | -3.75547% |
| w=1,h=.3,t=.01 mm;5 GHz | .2020746996 | .1793936687 | .1838990949 | .1847046612 | -2.44994% | -2.87540% |

Units are Np/m. Both also pass the unchanged **5% numerical-reference** target
separately for signal and ground surface Grams. The native final node counts
remain 128973/137643, using the same explicit 300k/1024 MiB admission as before.
`r1_fixed_cases_match_independent_boundary_panel_reference` passed in 2.93s
(after 9.18s incremental focused compile), log
`debugging/line-losses/bem-vs-native-review.log`.

The original Qucs implementation was rechecked: Rs, corrected vacuum impedance,
Ki, dimensions and the factor 1/2 are consistent. Both signals exceed 10 skin
depths, and ground loss is included independently. Qucs' Ki is an approximate
current-distribution factor; its published thickness eligibility is not a
universal stated 10% accuracy bound for arbitrary sharp rectangles. In fact,
as t/w tends to zero the exact normalized geometric conductor loss has a
logarithmic edge divergence, whereas this Ki approximation remains finite
because its only thickness dependence is through an impedance correction that
vanishes with t. This is a structural limitation of the approximation, not a
correction factor to apply to FEM. The independent values put Qucs above the
estimated physical limit by approximately 9.8%/9.4%, while finite FE recovery
is below it by 3.8%/2.9%; the original combined 12.35%/11.22% discrepancies are
therefore retained and explained, not made to disappear.

**Reviewer decision 2026-10-23, now applied to task and tests:** keep the original Qucs
results as a visible engineering comparison, and use the independently
validated boundary-panel reference with the existing 5% numerical criterion
for physical acceptance of these two exact cases. The former Qucs assertions are now engineering diagnostics, and the physical
assertions compare the same inputs with the independent numerical reference at 5%.
This change follows the explicit reviewer decision; no coefficient, material,
geometry, numerical tolerance or cap was changed.

### Boundary reaction recovery experiment

`native/analyze/tests/line_losses_review.rs` computes a consistent 1D boundary
mass solution on exactly the same FE fields, with a diagonal-preconditioned CG
solve. It conserves every total signal/reference current and uses the full
quadratic mass integral. This is diagnostic-only; production still uses the
reviewed lumped recovery.

On the finest identical FE fields, consistent recovery gives alpha .7819224047
and .1928836606, compared with lumped .7007235918 and .1793936687. Its apparently
better Qucs agreement is misleading: it lies approximately 7.4%/4.4% above the
independent corner-limit estimates. Coarser results oscillate around those
values. Therefore replacing lumped by consistent mass solely to pass Qucs
would not be scientifically justified. The experiment explicitly retains one
512 MiB coupon-budget refusal (planned 547366912 bytes) rather than raising a
global cap. The full diagnostic run passed after representing this anticipated
refusal explicitly; `recovery-review-final.log` records all values and currents.

## R2: reviewer-authorized replacement criterion

The original requirement for a fully specified published pair reference covering
both modes has **not been satisfied literally**. The reviewer explicitly replaced
it on 2026-10-23 with an independently converged numerical full-matrix comparison
at two coupling strengths, plus an accessible published odd-mode conductor-R
engineering comparison with declared nominal-thickness uncertainty. BEM is not a
published reference. The reviewer closed this equivalent criterion on 2026-10-23 after assessing the
full evidence set. An approval-flag-only Node test was removed as requested;
`independent_pair_surface_and_mutual_resistance` performs the physical acceptance
calculations in the ordinary Rust suite, with no skip.

### Main quantitative comparison: full power and both modes

The independent panel generator above was extended to two conductor potential
right-hand sides. The inverse of its independently integrated capacitance maps
these to unit signal-current bases. Every physical signal surface and the entire
infinite ground remain separate. The inputs were fixed before calculation:
w=.3, h=.18, t=.035 mm, sigma=58e6 S/m, f=1 GHz, epsilon=1, with edge gaps
.035 mm (strong) and .6 mm (weak). The histories use 80/160/320/640 horizontal
panels per face, no clipped corners, and retain all raw C, R and surface matrices.

The reviewer approved the same 5% physical norm for every matrix comparison:
`max_i |iᵀ(Rnative-Rref)i|/(iᵀRref i)`, over all nonzero currents. This equals
`||Rref^(-1/2) (Rnative-Rref) Rref^(-1/2)||₂` on its positive range. A changed
reference nullspace gives unbounded relative error; it is not masked by a small
denominator. The focused test checks zero/nullspace handling and aggregated small
entries. The same norm is also applied to each individual surface Gram. Absolute
and entrywise errors remain diagnostics, not claims that every small coefficient
is accurate to 5%. P7a and production convergence normalization remain unchanged.

| Gap mm | Native R11 / R12, ohm/m | Finest raw BEM R11 / R12 | Full-R power error | Largest surface power error | Even / odd alpha error |
| --- | --- | --- | ---: | ---: | --- |
| .035 | 33.06450859 / -3.77891582 | 33.68007541 / -4.08928462 | 2.45155% | 2.57200% | -1.02877% / -2.37883% |
| .6 | 23.98380896 / 1.37371042 | 24.48697792 / 1.36509577 | 2.21342% | 3.16505% | -1.82682% / -2.10655% |

The largest individual ground power errors are .09943%/.06293%. Each modal R
also passes 5%. Modal attenuation is independently formed from BEM modal C0,
`Zvac=sqrt(mu0 epsilon0)/C0mode` and `alpha=Rmode/(2 Zvac)`, rather than reusing
native impedance. All physical current bases and return surfaces are preserved.

The strong raw mutual difference is +.31036880 ohm/m: its magnitude error is
**7.589807%**. Against the separately recorded corner-limit mutual estimate
-4.19076318 ohm/m, it is **9.82750%**. The original scalar-mutual red probe is
retained in `debugging/line-losses/bem-pair-two-gaps-vs-native-admitted-review.log`
and the versioned diagnostic fixture. It is not deleted or renamed a scalar
5% success. Cancellation between negative signal mutual and positive ground
mutual makes that scalar condition stricter than the approved power criterion.

Final BEM power refinement changes are .48646%/.30932%; estimated remaining
power tails to the corner limit are .81741%/.52223%. The corner-limit estimate
has declared 1.5% estimated uncertainty, not a rigorous bound. The strong raw
mutual remaining-tail estimate is 2.42148%, so the 1.5% statement is **not** an
entrywise bound on the raw reference. Raw finest values remain the nominal
comparison. Corner estimates, per-entry tail estimates and histories are saved
separately. The finest admissible strong FEM uses 197278 nodes, step .625 um;
requesting .001 mm initial maximum edge refuses at 315707 planned nodes under
the unchanged 300k/600k caps. The weak case uses 109870 nodes. No cap was raised.

The earlier additional gap=.2 mm check and its signed contributions remain in
the fixture/history: native/full reference self -0.82567%, mutual +0.77713%, with
one signal mutual Gram about -5.39%. It is not a published reference.

### Supplementary published comparison: Johnson Table 6.1

[Johnson and Graham, chapter 6, Table 6.1](https://ptgmedia.pearsoncmg.com/images/013084408X/samplechapter/013084408X_ch06.pdf),
printed page389 (PDF page27), supplies four 1-GHz differential microstrips and
loop conductor resistance from a method-of-moments magnetic simulator with
120 segments per trace and linear current variation. The author estimates
accuracy +/-2%; this is an author estimate, not a rigorous error bound. Dimensions, conductivity
5.98e7 S/m and nominal one-ounce trace construction are available, but exact SI
thickness and reference-plane thickness are not. Thus it is supplementary
engineering evidence with explicit uncertainty, not an exact numerical oracle.
The normalization is `Rloop=2*(R11-R12)=2*Rodd`; its consistency with the printed
100-ohm attenuation columns was checked independently.

Before evaluating results, nominal thickness endpoints .03429/.03556 mm
(1.35/1.4 mil) were declared, based on
[Bentley, Copper Foil Weight vs Thickness, page1](https://www.upmediagroup.net/publications/WhitePapers/Divsys_COPPER_FOIL_WEIGHT.pdf).
These are nominal conversion conventions, **not manufacturing tolerance bounds**.
All four table rows and both endpoints were evaluated. No endpoint was selected
by its result. The explicit .03556-mm ground thickness is a thick-plane modeling
hypothesis because the publication omits it. The publication's nonmagnetic mask
is irrelevant to the magnetic current distribution for conductor R; no unmasked
C, Z or attenuation is compared with its masked results. All hypotheses and
source inputs are saved in `johnson-reference.json`.

| Published w / edge gap, mil | Published loop ohm/m | Native at 1.35 mil | Error | Native at 1.4 mil | Error |
| --- | ---: | ---: | ---: | ---: | ---: |
| 8 / 30 | 60.7 | 62.65916216 | +3.22761% | 62.13599939 | +2.36573% |
| 7 / 11 | 61.7 | 65.01489949 | **+5.37261%** | 64.36323775 | +4.31643% |
| 6 / 7 | 66.5 | 68.71722236 | +3.33417% | 68.01360318 | +2.27610% |
| 5 / 5 | 74.4 | 77.40717749 | +4.04191% | 76.52689061 | +2.85872% |

Every row has h=5 mil. The 5.37261% result is retained as a failed 5% diagnostic;
no thickness fitting or tolerance widening was used. The focused supplementary
test checks all eight inputs complete and records comparisons, rather than
claiming an exact 5% table gate. The main quantitative gate remains the independent
BEM comparison above. The reviewer accepted this explicit evidence set as the replacement criterion.

### Other literature investigation and limitations

Verma–Nasimuddin2003 and Gentili–Melloni1995 author PDFs were located, but normal
public downloads returned403 and browser image responses exposed no pixels for
complete input transcription. No inaccessible figure was invented or digitized.
Bernal–Medina–Horno1997 was downloaded and inspected: TableIII's .23606 is near
the interpreted signal-only .23426, while including ground gives .34179. Its
normalization is unresolved, so it is not counted as an independent benchmark.
Its four/five-conductor examples cannot be made pairs by deleting conductors.
The original literal published-both-mode gap is preserved in this history.

## Small requested diagnostic correction

`Configuration::screen` now distinguishes a nonrepresentable/nonpositive delta
(numeric overflow/underflow) from a finite delta exceeding dimension/10. Both
retain `SECTION_SKIN_DEPTH`. The focused unit test verifies the distinct reason
and that neither physical eligibility nor the 10-delta guard was widened.
It passed (`skin-diagnostic-review.log`). The rebuilt addon now emits the corrected text; numeric overflow/underflow and
finite dimension-limit native smoke checks pass, followed by a normal solve.

## Sources and reproducibility

- [Qucs primary technical reference](https://qucs.sourceforge.net/tech/node75.html),
  equations11.80–11.83; the existing unchanged generator remains versioned.
- [Wheeler1955, original article scan](https://www.worldradiohistory.com/Archive-IRE/40s/50s/IRE-1955-07.pdf),
  Skin Resistance of a Transmission-Line Conductor of Polygon Cross Section.
- [Bernal et al.1997 institutional PDF](https://idus.us.es/bitstreams/aae30e40-44f5-4243-9c30-35d64123f0e5/download),
  DOI10.1109/22.622930; saved PDF/rendered pages under ignored
  `debugging/line-losses/review-sources/`.
- [Verma–Nasimuddin2003 publisher record](https://doi.org/10.1002/mop.11076)
  and [author-uploaded full text](https://www.researchgate.net/publication/227921380_Determination_of_conductor_loss_of_multilayer-coupled_microstrip_lines_for_CAD_application).
- [Gentili–Melloni1995 publisher record](https://doi.org/10.1109/22.390183)
  and [author-uploaded source](https://www.researchgate.net/publication/3119216_The_incremental_inductance_rule_in_quasi-TEM_coupled_transmission_lines).

Commands: `python tests/fixtures/analysis-line-losses/boundary-panel-reference.py`;
`CARGO_BUILD_JOBS=1 cargo test --release --manifest-path native/analyze/Cargo.toml --test line_losses_review -- --test-threads=1 --nocapture`;
focused unit test name `skin_depth_numeric_range_reason_is_distinct_from_dimension_limit`.
All generated reference values remain separate from production coefficients.


## Final revision verification

`npm run native:build` passed with CARGO_BUILD_JOBS=1 in17m48s. Targeted native
smoke verifies the numeric reason, finite dimension reason and subsequent normal
2-frequency completion. Final ordinary release Rust review suite:5 passed,
0 failed/ignored (22.37s). Final Node fixed-reference tests:2 passed,0 skipped
(3.97s), against the rebuilt addon. No full package/corpus/board replay was repeated
for this diagnostic-only delta, as explicitly authorized by the reviewer.

Current addon SHA256:
`5cc47581c5bc2efa6e6817bea2af3cb10479dc0b0e5210337571ced6ff565a4a`.
Current losses.rs SHA256:
`03cf01a9eab1c0c450e55c5dee58f2fb42122ad722bece11083abe01a7abca11`.
Historical reports/package remain bound to addon
`de11d290f1ab5aade192bd8d08f4cc2619fd554c658a2ace355392eebd529a25`.
All39 historical worker/log/static-image hashes were rechecked unchanged.
`debugging/line-losses/review-revision-evidence.json` records current sources,
logs and line-ending-only hash differences separately. The historical evidence
file is unchanged. Interactive HTML controls remain unverified; static plot QA
is recorded separately and is not described as browser screenshots.
