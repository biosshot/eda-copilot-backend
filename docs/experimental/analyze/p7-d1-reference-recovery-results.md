# P7-D1R joint-reference current recovery — NO-GO, 2026-10-31

**Final status: NO-GO for the fixed slot joint reference candidate.** The `0.4162806829 A` return flux is not a reduction, source-mapping, area, unit, or complex-phase artifact. The same deficit is already present in the original full-space harmonic DC current lift. The mixed P1-potential / independent constant-vector-current formulation enforces weak nodal divergence, but does not impose normal-current continuity on tetrahedral faces; its raw cell density is therefore not a conservative physical section-flux certificate around the slot. A minimal local implementation correction is not justified: conservative current recovery/formulation and full-space current verification are required. The existing `10.2046289%` magnetic patch sensitivity also remains unchanged and independently prevents reference acceptance.

This is not D1/P7 acceptance, not a conclusion about hybrid feasibility, and not a new solver campaign. No hybrid/interface moments, geometry, physical guards, tolerances, or native production code were changed.

## Inputs, provenance and fixed diagnostic contract

Starting HEAD was `db535f0633835ac38a61eca322b467e8d34e6a0d`; the parent-owned task file was left untouched. Only saved prepared/reduced states were used. No field assembly, training, magnetic solve, STM32, or direct-DSL campaign was run.

| Item | Value |
| --- | --- |
| Slot fixture SHA256 | `f6d066e474f70e9f68388abe08c2f54606148b6792377c7748630869664613f5` |
| Continuous fixture SHA256 | `3cb149436375a86340d9ac4466e509d9f9853d46793f2b859c9cc741b554ebf0` |
| Slot saved `joint-fine.json` SHA256 | `d878292e731ba48e6d319c70a500b1bc8c54c280b2f719341dde72f803224033` |
| Continuous saved `joint-fine.json` SHA256 | `3c15adb7cdff5098651b91c4b5e49838fc67315844d57972cdf46abe02a9a1c4` |
| Recovery addon SHA256 | `f15e8aee3c87da0847902c571199a9bc2eaa5f116bf3bb957f686fb9d727ee73` |
| Slot original packet/core identity | `debugging/p7-closure/1793437157149686200/` / `d63946281d8dfb61c14d71bbbe0f3dc565d52c1b4eb2944fede9ccf1cb08d0a0` |
| Continuous original packet/core identity | `debugging/p7-closure/1793438893314147000/` / `c6a39ed731e92cbf68449cb5fb6683f9127125f2b1d3619cae3e0a94bef90048` |

The fixed excitation is the existing four-port RMS Norton drive `d=[1,0,-1,0] A`, with `50 ohm` on every port. Sections were fixed before this audit at `x=0.500123`, `1.000123`, and `1.500123 mm`, all using physical normal `+x`. The diagnostic absolute current floor is `1e-6 A`; no pass tolerance is assigned to the defective reference. Negative return flux denotes physical current in `-x`.

For complex port voltage `V_p=V'_p+jV''_p`, the intrinsic current entering positive port electrode `p` is

`I_p = d_p - V_p/50`.

The expected `+x` return flux obtained independently from the left and right electrode balances is

`I_return,left = -(I_0+I_1) = -1+(V_0+V_1)/50`,

`I_return,right = I_2+I_3 = -1-(V_2+V_3)/50`.

The two expressions agree to roundoff. All voltage/current phasors are RMS; native mean Joule power is `sum R*(Re(I)^2+Im(I)^2)`, with no additional peak/RMS factor.

For every tetrahedron intersecting a plane, the audit exactly intersects its six edges, sorts the resulting polygon in the physical `yz` plane, computes its area in `m^2`, and evaluates

`Phi_x(x) = sum_t A_t(x) J_t,x`, `J_t = i_t * cellDensityPerAmpM2_t`.

Thus `i_t` is in A, density in `1/m^2`, and flux in A. The same code selects every cell whose mapped source is on BOTTOM, including both bypasses. Slot has `126` BOTTOM copper cells and continuous has `90`; neither has an empty source list or unknown layer. At the slot interior section, `12` intersected BOTTOM cells have total area `1.900000050663936e-9 m^2`; continuous has `30` cells and area `5e-9 m^2`.

## Complex section evidence

The notation below is `real + j imag`, in A RMS. The expected slot balance is `-0.9995544314890277 + j 6.168940268411154e-8 A`; continuous is `-0.9995656894569854 + j 5.981353630029737e-8 A`.

| Case | x (mm) | Recovered BOTTOM flux (A) | TOP flux (A) | Sum over all conductors (A) |
| --- | ---: | ---: | ---: | ---: |
| Slot | 0.500123 | `-1.073463527460 + j 0.000001350989` | `0.999553026812 - j 0.000000188319` | `-0.073910500648 + j 0.000001162670` |
| Slot | 1.000123 | `-0.416280682890 + j 0.000005691491` | `0.999554430796 + j 0.000000244968` | `0.583273747906 + j 0.000005936459` |
| Slot | 1.500123 | `-1.071452059341 + j 0.000000468441` | `0.999552988044 - j 0.000000119206` | `-0.071899071297 + j 0.000000349235` |
| Continuous | 0.500123 | `-0.999570032844 - j 0.000002005077` | `0.999552186731 + j 0.000001496445` | `-0.000017846114 - j 0.000000508631` |
| Continuous | 1.000123 | `-0.999564993267 + j 0.000000649387` | `0.999567418776 - j 0.000000689767` | `0.000002425509 - j 0.000000040380` |
| Continuous | 1.500123 | `-0.999561240928 + j 0.000002213042` | `0.999579454590 - j 0.000001710162` | `0.000018213662 + j 0.000000502880` |

At the slot interior, the real branch contributions are `-0.372640104632 A` in the negative-y bypass and `-0.043640578258 A` in the positive-y bypass. Their sum is the defective `-0.416280682890 A`; the imaginary components cannot supply the missing real current. The slot interior real deficit is approximately `58.35%` of electrode balance. Even before and after the hole, the raw section flux differs by about `7.4%` and `7.2%`, respectively. The continuous control agrees with electrode balance at all three sections to approximately `1.9e-5 A` or better.

## Full harmonic state, reduction and `sigma E`

The audit independently assembles the saved small dense descriptor `(G+j omega E)x=Bd`, including the same four `50 ohm` loads, and compares it with native `solveLocal`. It then reconstructs full physical currents with saved `Q` and full P1 potentials with saved `voltageBasis`. For each tetrahedron it computes `grad(V)` from the three nodal differences and obtains native conduction coordinates `(-length*grad(V))/R`; this is exactly `J=sigma E` after multiplication by cell density.

| Case | Frequency | Max native versus dense/Q current error (A) | Relative recovered-current versus `sigma E` norm |
| --- | ---: | ---: | ---: |
| Slot | DC | `8.307132759e-12` | `4.548910946e-11` |
| Slot | 1 kHz | `7.327471978e-15` | `1.299875995e-4` |
| Continuous | DC | `2.837863278e-11` | `5.064478924e-11` |
| Continuous | 1 kHz | `9.567867363e-15` | `1.294812278e-4` |

Dense and native port voltages also agree to roundoff. At DC, the slot uses only the original three harmonic current columns: the maximum coordinate in every later training/magnetic current mode is `8.231316745e-13`; continuous gives `1.733860522e-11`. Zero-gradient potential bubble coordinates remain gauge freedoms and are not used as evidence. Consequently, the defective slot section flux is already the flux of the original full-space harmonic conduction solution, not a loss introduced by reduced reconstruction.

At 1 kHz, `sigma E` and recovered current differ by the expected small inductive constitutive correction, but their real section flux remains approximately `-0.4162806835 A`; postprocessing the electric state instead of current recovery does not repair it.

The saved packets have `verificationStatus:not-run` and `fullChecks:[]`. The exact full-space loaded 1 kHz potential/current arrays are not saved, so no full-versus-reduced 1 kHz error is claimed. A new full FEM rerun is not needed for this NO-GO: the saved full harmonic DC state already exhibits the blocking defect. Any later acceptance still needs explicit full-space loaded AC current checks.

## Native current-space localization

`volume_current.rs` assigns an independent constant three-vector current to each copper tetrahedron and P1 potentials to nodes. Its normalization is consistent: with `length=cbrt(volume)`, `density=1/length^2`, `R=1/(sigma*length)`, and `incidence=-length*grad(N)`, the assembled electric stiffness is the standard `sigma*volume*grad(N_i).dot(grad(N_j))`, and recovered density is `sigma E`.

However, `divergence()` accumulates only P1 nodal test reactions. There is no tetrahedral-face normal-current continuity constraint or H(div)-conforming recovery. Therefore a small weak/global KCL residual and exact electrode balance do not imply that integrating raw discontinuous cell `J_n` over an arbitrary physical plane gives the same current. The slot's nonzero all-conductor section sum (`+0.5832737471 A` at the interior) is a minimal reproducible counterexample to treating that raw density as a conservative return-current reference.

This is not an absent-branch basis problem. On the saved slot reduced basis, the two bypass-flux functionals have singular values `8.299198824` and `1.763354093 A` (rank 2). The before/inside/after BOTTOM flux functionals have singular values `19.52884769`, `17.82978607`, and `7.196687577 A` (rank 3). Thus the basis can represent unequal branch currents and longitudinal redistribution; representability does not impose physical continuity. Slot and continuous use the same native construction and audit, with only their existing geometry/mesh differing.

The evidence does not establish that mesh refinement alone will cure the defect, nor quantify continuum error. The current mesh is unresolved for this physical flux certificate, and the prior magnetic patch1 to patch2 sensitivity remains `10.2046289% > 2%`. Domain sensitivity and magnetic refinement were not rerun because the numerical path did not change.

## Changes, commands and verification

Added diagnostic-only files:

- `tests/fixtures/analysis-p7-joint-section/capture-reference-recovery.ts` — reduced native recovery at DC and 1 kHz with source/addon hashes; SHA256 `40f99bc41e158f8c62d1ef5f0c5b6d52a4271b9def518590dab43e5b08de7c63`.
- `tests/fixtures/analysis-p7-joint-section/audit-reference-recovery.py` — independent dense reduced replay, complex section integration, source coverage, basis functionals, and `sigma E`; SHA256 `c394d0912e3431b7cc364f991407276aaf039852a3efcc8b95a0d6b536a40555`.
- `tests/fixtures/analysis-p7-joint-section/plot-reference-recovery.py` — saved-audit plots; SHA256 `98cff887fa39c8befc87c5a5c0235be54d331db632844940a5e49c9c7b289ae4`.
- This report. No native or production TypeScript diff.

Final clean reproduction from the repository root:

```sh
node --import tsx tests/fixtures/analysis-p7-joint-section/capture-reference-recovery.ts debugging/p7-closure/1793437157149686200/joint-fine.json debugging/p7-d1-reference-recovery/slot-recovery.json
node --import tsx tests/fixtures/analysis-p7-joint-section/capture-reference-recovery.ts debugging/p7-closure/1793438893314147000/joint-fine.json debugging/p7-d1-reference-recovery/continuous-recovery.json
python tests/fixtures/analysis-p7-joint-section/audit-reference-recovery.py debugging/p7-closure/1793437157149686200 debugging/p7-d1-reference-recovery/slot-recovery.json debugging/p7-d1-reference-recovery/slot-audit.json
python tests/fixtures/analysis-p7-joint-section/audit-reference-recovery.py debugging/p7-closure/1793438893314147000 debugging/p7-d1-reference-recovery/continuous-recovery.json debugging/p7-d1-reference-recovery/continuous-audit.json
python tests/fixtures/analysis-p7-joint-section/plot-reference-recovery.py debugging/p7-d1-reference-recovery/slot-audit.json debugging/p7-d1-reference-recovery/continuous-audit.json debugging/p7-d1-reference-recovery/reference-current-audit.png
npm run typecheck
```

The complete clean replay/plot sequence took `6.398 s`; typecheck passed in `6.022 s`. These are small reduced/postprocessing runs, not heavy FEM jobs; no OS memory profile is claimed. All generated artifacts are under `debugging/p7-d1-reference-recovery/`. `reference-current-audit.png` was opened and visually inspected: slot recovered and `sigma E` curves coincide, continuous remains near electrode balance, and the all-conductor plot exposes the slot interior imbalance. No native build was run because native code did not change; the addon remains the parent-reviewed final `f15e8aee...` binary.

## Exact blocker and stop condition

Before this slot candidate can be a numerical reference, provide a mathematically conservative normal-current representation or equilibrated recovery on the conforming mesh, with explicit full-space loaded DC/1 kHz states and complex section-current checks before/inside/after the hole. Demonstrate that the representation preserves electrode balance without empirical scaling and separately pass the unchanged magnetic patch/domain gates. This is current formulation/recovery work, not a justified one-line patch in reduction or a license to start a solver campaign.

Stop here for parent review. No D1/P7/P8/P11 status is closed, and no hybrid/interface work was started.
