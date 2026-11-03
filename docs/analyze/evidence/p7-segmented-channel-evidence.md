# Actual section/local channel continuation

Status: implementation and verification in progress; **whole P7 is not accepted**. This supersedes the earlier statement that no longitudinal assembler exists. It does not supersede the original acceptance requirements.

## Implemented path

The existing no-scope `.channel({from,to})`/`method('layered')` selects a physical island, retains its branches and all neighbouring copper in the candidate domain, and constructs longitudinal slabs. Polygon edges, pads, bores and material changes force joint local regions; captured constant sections use the actual numerical polygon FEM. Close changes merge, rather than selecting a shorter electrical route. Original board snapshots are unchanged. Every selected source/layer/barrel has disjoint coordinate-interval ownership; exact cut-side finite electrodes connect the slabs. Multiple disconnected physical return islands remain distinct, regardless of net labels.

`segmented-channel.ts` calls real section and local producers, creates physical SI RLC section descriptors, and assembles their matrices through mapped P6 terminals. Multiway branches share actual conductor nodes. Independent cut returns do not become a shared ideal ground. Full mutual matrices, not independent two-port cascades, are retained within each slab/section. Positive local feedback does not certify omitted inter-slab electromagnetic interaction.

The automatic low-band section approximation currently uses uniform DC ribbon R and thin-ribbon internal L, screened at skin depth greater than ten foil thicknesses. Its common band is **1–10 kHz**. Local HF, proximity/transition verification and temporal admission are absent; out-of-band requests fail instead of extending this approximation to RF. Reused section coefficients/local models keep existing source/material/core identities and bounded caches. Default budgets remain unchanged; explicit developer limits are recorded separately.

Whole assembled spatial/domain checks retain the original 2% S50 and intrinsic copper loss gates, with wave floor 0.01 and loss floor 1e-18 W. Adjacent-frequency transmission phase slopes additionally screen delay with a 1 ps floor. Air-domain expansion includes transverse height. These are sensitivity checks, not a complete 5% error certificate or continuous-band causality proof.

## Actual coupon and reproducibility

`tests/helpers/pcb-analysis-segmented-coupon.ts` contains the exact ordinary DSL. The owned fixture is `tests/fixtures/analysis-segmented-coupon.json`: 10 mm signal/finite return, physical end contacts, 0.1 mm dielectric and 0.01 mm Cu. It automatically yields two local FEM end regions and one 9.32 mm polygon FEM section. All computations execute the existing native library and P6.

```
python scripts/verify-p7-automatic-supervised.py --scenario segmented-coupon --seconds 600 --native-memory-mib 2048 --tree-memory-mib 2560 --reserve-mib 512 --warm
```

Saved `debugging/p7-channel-plan/automatic-supervised-1793362436862100200` has an empty-cache cold call 103.704906 s and same-worker repeat 1.151818 s; total supervised wall 105.927378 s, tree peak 534663168 B, largest child 465719296 B. The repeat reused disk local models and memory section results with zero local assembly/training. Spatial maximum change 0.422352%, domain maximum change 0.849622% (delay dominates); original loss/wave gates pass. This packet predates the final source-surface refinement guard and is identified by its recorded core hash; it is not a final-binary measurement.

Uniform two-section numerical FEM composition versus an unpartitioned line, a three-arm numerical star and a fully coupled four-port section composition have separate real numerical tests. Copper overlap/gap/nonlocal-group and delay-gate negatives are included. Geometry-only actual STM32 tests validate the new slab plan and retained D5/JP2 branches; they are not electrical acceptance.

## Independent accuracy failure retained

The complete independently extracted coupon is **not an accepted exact reference** until its spatial/domain/current errors are quantified. Nevertheless, comparison already prevents claiming 5% success. With floors fixed before evaluation (S 0.01, delay 1 ps, Cu loss 1e-18 W), original coarse full-volume delay 76.4 ps differs from composed 64.1 ps by 16.13%. An independent XY field refinement preserving the original current copper gives 70.1 ps at 0.3 mm and 69.7 ps at 0.15 mm. The latter changes delay about 0.6% while the composition error remains 8.0851%; complex S error 0.0006714%, Cu-loss error 0.0030117%. Small S error alone cannot admit the delay.

The 0.3/0.15 mm reference runs are saved as `automatic-supervised-1793362597950723400` / `automatic-supervised-1793362633856079400`: cold 26.592941 / 22.522274 s, tree peaks 556302336 / 792145920 B, native field factors 29860 / 60380 unknowns. Currents remain 216 on the original 28-potential mesh. This establishes a real unresolved composition/interaction error, not a resource-only failure. No calibration factor or fitted parasitic is added to hide it.

`audit-p7-segmented-reference.ts` writes each retained comparison and returns nonzero on FAIL. `render-p7-segmented.py` plots actual ownership, transmission, phase delay and errors; generated figures stay in ignored output and were opened for inspection. Earlier 3D field-refinement prototypes were removed after measured element proliferation; the original historical replay path is unchanged.

## Still required by the active assignment

Verify/correct inter-slab field coupling against the independent whole-channel reference, then actual STM32 and positive BandAmp channels, the geometric obligation matrix, TPS finite-front/DC/common-band admission, true local HF surface/volume-transition operators, continuous-band/temporal error and final cold/warm/restart/invalidation evidence. A small numerical line or DC-resistance success does not close these obligations. P8/P11 gain an internal actual assembly mechanism, but **no new certified arbitrary extracted-channel or physical transient dependency**.

## Further independent level and real-channel progress

`automatic-supervised-1793363430965924500` completed the 0.075 mm independent field reference: 121500 field unknowns, cold 41.961877 s, tree peak 1312108544 B, native OS peak 1246867456 B, 3100 MiB native / 3200 MiB tree / 512 MiB reserve / 600 s. Original 28-potential/216-current copper and contacts remain unchanged. Final phase delay is 69.48 ps; the composition still fails at 7.7902%, despite complex-S error 0.0006325% and Cu-loss error 0.0030117%. The 0.15 to 0.075 mm delay change is about 0.32%, so another mesh-only refinement is not a justified solution to the composition defect.

A real new-path U5.7 -> J5.2 run (`automatic-supervised-1793363235134322800`) reached geometry and refused a real drilled connector padstack: 20.716751 s, tree 288587776 B; 3100 MiB / 1800 s developer limits were not exhausted. The ordinary through-via-only restriction is being extended to explicit round through-pad stacks, using the original pad-local offset/rotation, unchanged native bore and coherent wall contour. NPTH retains its void without a barrel. A numerical DC/control test verifies physical through continuity and the absence of any net-label connection for NPTH. Plated slots remain explicitly unsupported, without substituting a round hole. The original board fixtures are untouched.

Focused numerical composition checks PASS: uniform two-section absolute complex-S error 1.404441e-10; coupled four-port error 8.357664e-7; analytic three-arm star, ownership negatives and automatic no-scope mixed channel. The mixed test's DC loop resistance is 0.202765007890 ohm versus independently known 0.202758620690 ohm (0.00315015% error). Its final focused run took 114.332 s including worker startup; that is a correctness run rather than an isolated supervised timing. The test explicitly checks finite distinct return nodes, real local P6 calls, the 2% gates, and temporal refusal.

## Final source verification and remote resource admission

Commits: `f9d8889` physical junction/cut foundation, `80b82d1` independent surface traces and explicit padstacks, `55704cd` actual automatic slab/P6 path, `5af5b07` increased developer geometry admission. Ordinary native build PASS 6m17s, addon SHA256 `7784d1e00a72103ea610bae920b052a79890e10592115f2f2d8c32ec9e30f771`. TypeScript typecheck/build and 20 focused adapter/cache/domain/plan/delay tests PASS. Native focused coverage includes unchanged authored STM32 geometry, physical contacts, round/NPTH DC, affine/charge-energy field, memory and terminal junction controls. The developer cap test retains the exact 2048 MiB production counts and enforces bytes for increased limits; no OOM guard was removed.

After round-pad support, the remote plan encountered a partially intersected neighbouring wall (`automatic-supervised-1793363964851526300`, 4.007972 s, 258822144 B tree). Automatic domain closure now includes the entire physical wall and iterates over newly encountered barrels. It then reached explicit old geometry caps (`automatic-supervised-1793364043002382300`, 15.224468 s, 348622848 B tree): 139601 nodes / 814320 tetrahedra. Their byte plan fits the increased developer limit, so the developer-only count admission was corrected instead of treating the old cap as P7 infeasibility.

`automatic-supervised-1793364555861516700` is the subsequent real remote cold attempt, 3100 MiB native / 3200 MiB tree / 256 MiB reserve / 1800 s. It prepared 183 material atoms, 13292 potentials and 99189 current coordinates; real global electric factor 99215 unknowns completed. Magnetic trace-ordering workspace requested 2615969204 B versus 2448936900 B remaining and was refused before allocation. Wall 28.170042 s, tree peak 1656131584 B, largest child 1572638720 B. This is execution through the new path, **not a positive remote electrical result**. A separately guarded 4096 MiB attempt follows when free RAM admits the full tree plus reserve.

The operator counterfactual script `audit-p7-channel-interaction.ts` retains original matrices and removes L only in a separate diagnostic copy. At 1 kHz composed/full original delays are 64.067/69.480 ps; zero-L delays are 24.278/27.055 ps. This does not certify an additive physical error split. It demonstrates that correcting a scalar length or magnetic term alone is not justified. P6 terminal welding enforces continuity but cannot generate omitted cross-block field energy. A joint electromagnetic coupling/consistent interface treatment is still needed before claiming complete-channel 5%.

The guarded 4096 MiB native / 4608 MiB tree / 512 MiB reserve remote attempt `automatic-supervised-1793364627871399200` completed electric preparation and symbolic magnetic admission, then refused the numeric factor: **8714661076 B requested versus 3493318596 B remaining**. Cold wall 30.536396 s, tree peak 2623897600 B, largest child 2540056576 B; no timeout/OS kill or 2 GiB cutoff. This exceeds even the native developer 8 GiB upper limit before other retained state; available host RAM at preflight was 5533286400 B. Raising to an unsafe/unavailable host budget cannot supply a positive real result. The remote plan correctly merges its dense changes into a joint local group; replacing it with an independent cascade to reduce memory would discard the very coupling that remains unresolved in the small coupon. No further identical factor attempt is justified.

## Error and dependency ledger

| Layer | Evidence | Admission |
| --- | --- | --- |
| Terminal orientation, SI, branches, mutual section matrices | real FEM/P6 two-section, star and coupled controls | numerical controls PASS |
| Automatic physical source ownership and return mapping | actual coupon + remote board planning, no authored scope | implemented; no network-name joins |
| Local numerical/residual/reduction | existing native gates and actual new local training | new whole-channel independent reduction holdouts not fully certified |
| Grid/domain sensitivity | real two-level magnetic and domain checks, field reference three levels | finite sampled controls only; 2% gates preserved |
| Assembled physical response versus separate whole geometry | 7.7902% delay discrepancy | **FAIL**, target 5% not met |
| RF and continuous DC/temporal common band | no verified joint surface/volume transition or causal representation | **not admitted** |
| Complete real STM32 | new actual path reaches magnetic numeric-factor plan, 8.715 GB factor | **not a positive result** |
| Complete real BandAmp / TPS finite-front dependency | existing partial evidence remains unchanged | **still required** |

The assembled low-band candidate does not grant new complete P8/P11 physical readiness. Existing conditional local/section dependencies remain as documented in the previous acceptance ledger. No P8/P11 feature implementation, arbitrary-GHz admission or laboratory accuracy claim is implied.

## Final-binary cold / warm / restart

Final addon identity above is unchanged. `automatic-supervised-1793364676764354600` starts with an empty cache and executes real FEM sections and local models in the ordinary no-scope DSL; total cold+same-worker-warm wall **119.403078 s**, process-tree peak **530907136 B**, largest child **462254080 B**. `automatic-supervised-1793364876088134200` is a separate fresh process restoring that directory: **2.568159 s**, tree **254885888 B**, largest child **175595520 B**. Both use explicit 2048 MiB native / 2560 MiB tree / 512 MiB reserve and finite 600 s per call; this is a small-model measurement, not a return to a universal 2 GiB refusal criterion.

Cold/warm/restart matrices are bit-identical; every warm/restart final block reports assembly 0 and training 0. Individual call walls and complete per-level cache evidence are saved in `cache-comparison.json` and the native producer reports. Final-binary comparison to the separately field-refined whole geometry returns nonzero FAIL with exactly the retained 7.7902% delay discrepancy. Its plotted transmission/segmentation/delay/error evidence is `channel-evidence.png`; the figure is generated from actual outputs, not a drawing of an intended result. Earlier partially warmed/cold attempts keep their own identities and are not merged into this timing.

No complete real-board 5% result is claimed. This is the actual implemented section/local mechanism and a reproducible physical counterexample to its current independent-field approximation. Whole P7, positive complete RF, TPS physical transient supply and complete extracted-channel P8/P11 readiness remain open.
