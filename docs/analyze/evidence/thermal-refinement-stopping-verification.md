# Thermal refinement stopping: saved evidence and blocker

Task664056e. Saved-array audit completed; production refinement optimization is **blocked** by the available field evidence. No new native build or full-board request was spent (0/2). Whole P9 and600s performance remain unaccepted. Historical0.1% temporal FAIL is unchanged.

## New temperature audit

The complete saved TPS ordinary/tighter1800s pair passes `abs(Tfine-Tordinary) <= max(0.1K, .01*abs(Tfine-Tinitial))`, with initial25C. The accepted physical-domain P1 helper covers both nodal sets, boundary vertices/midpoints and inward traces at all seven event/output times, all three layers and compact nodes: **599424 scalar observations, zero violations, missing0**. Final meshes match. Original physical/source/energy/residual/resource audits were also applied to the actual saved reports.

Maximum allowance utilization is **0.02230378324297301** at U1:case60s: ordinary73.48846873728924C, tighter73.49928591289364C, delta0.010817175604401541K, allowance0.4849928591289364K. Maximum absolute difference remains U1:junction60s,0.010969778009652487K. This is separate from the original temporal0.1168551214% >0.1% FAIL; the old report and arrays were not overwritten. Cooling uses absolute heating magnitude; current ambient and absolute Celsius/Kelvin are not normalization substitutes.

New negative checks reject an actual-witness coarse-temperature substitution at1.01 allowance, current-ambient/Celsius/Kelvin substitutions, nonfinite normalization and missing coverage. Removing half the actual coarse triangles gives8952 missing physical and6529 missing boundary samples and rejects. An initial attempt removing only the first triangle did **not** create missing sampled coverage (adjacent triangles still covered these samples); that failed fixture is retained as `negative-fixture-no-missing-first-triangle.json`, not counted as a passing negative. Original26 physical substitutions remain separately preserved in the extended packet.

## Saved stage costs and changes

Each elapsed cost includes solve, extraction and comparison for that stage. These scalar histories are complete; intermediate full temperature fields were not saved. Pointwise convergence cannot be reconstructed from a global maximum and a different global rise. A saved concrete failing witness proves failure; maximum absolute delta<0.1K is sufficient to prove the new temperature inequality for all sampled field points (compact changes checked separately). Absence of full intermediate fields is explicit.

### TPS ordinary600s

|Stage|Role|Seconds|Nodes|Max field delta K|Old global spatial change|Missing|
|---:|---|---:|---:|---:|---:|---:|
|0|seed|42.930|18300|0.000000|0.0000%|—|
|1|directional|71.057|25527|4.966753|21.1306%|0|
|2|directional|87.896|30900|3.706894|17.6820%|0|
|3|directional|92.862|33093|1.790768|7.0211%|0|
|4|directional|83.553|33438|0.565108|2.4645%|0|
|5|directional|80.538|33492|0.351461|1.5084%|0|
|6|independent-control|106.812|42660|0.457613|1.5467%|0|

Source: `debugging/thermal-verification/TPS63060DSCR-2026-10-24T10-53-36-958Z-thermal-final-time-zi5hXc-checkpoint.json`.

### TPS tighter1800s

|Stage|Role|Seconds|Nodes|Max field delta K|Old global spatial change|Missing|
|---:|---|---:|---:|---:|---:|---:|
|0|seed|79.250|18300|0.000000|0.0000%|—|
|1|directional|112.868|25527|4.968204|21.1311%|0|
|2|directional|141.708|30900|3.707876|17.6824%|0|
|3|directional|154.895|33093|1.791212|7.0211%|0|
|4|directional|154.897|33438|0.565143|2.4645%|0|
|5|directional|167.856|33492|0.351581|1.5085%|0|
|6|independent-control|206.165|42660|0.457697|1.5467%|0|

Source: `debugging/thermal-verification/extended-convergence/checkpoint.json`.

### Band ordinary600s

|Stage|Role|Seconds|Nodes|Max field delta K|Old global spatial change|Missing|
|---:|---|---:|---:|---:|---:|---:|
|0|seed|44.543|21397|0.000000|0.0000%|—|
|1|directional|57.962|25420|1.291821|28.8197%|0|
|2|directional|56.919|29659|0.903352|20.2243%|0|
|3|directional|58.621|31312|0.424131|9.4504%|0|
|4|directional|59.274|31570|0.111011|2.4735%|0|
|5|directional|59.086|31615|0.081370|1.8283%|0|
|6|independent-control|88.471|42877|0.059444|1.3243%|0|

Source: `debugging/thermal-verification/BandAmpV5-2026-10-24T11-14-25-916Z-thermal-final-time-W8pzml-checkpoint.json`.

## Stopping decision before any new run

The premise that current native code necessarily exhausts local maxPasses is false. `native/analyze/src/thermal/refinement.rs:951` derives `candidate_pass` from the full physical comparison; line986 immediately constructs the independent nested global control whenever it passes, at any pass. Line953 uses maxPasses only to stop an unconverged candidate. Independent-control failure returns honest unconverged. No board name or step-dependent condition is involved.

TPSstage5 saved absolute witness at TOP(10.039081029862707,2.227724999189377)mm,t60: coarse37.78251562522266C, fine37.43105468061083C, delta0.35146094461183K >new allowance0.1243105468061083K. Its old global spatial1.50844% passes. TPS independent stage6 also fails the new pointwise rule: BOTTOM(-11.422002054571744,2.1383000016212463)mm,t60, coarse39.153954199790405C, fine39.61156705857126C, delta0.45761285878085K >allowance0.1461156705857126K, while old global spatial1.54672% passes. Earlier directional stages have still larger failing absolute witnesses. Thus the new temporal pair PASS does not establish new pointwise **spatial** convergence.

Bandstage4 TOP(1.838212509,2.739106238)mm,t60 has delta0.111010780K >allowance0.1K. Stage5 max field delta0.081369773K and compact delta1.75e-7K satisfy the new rule; stage6 independent delta0.059444451K also satisfies it. But existing code already transitions atstage5: there is no demonstrated saved-stage reduction. Band temporal acceptance remains open.

The acceptance requirement for a future candidate is: transition on demonstrated field stabilization, retain mandatory independently generated nested global control, require original spatial2% and new pointwise temperature criteria separately, preserve coverage and physical/resource guards. A compact plateau/local indicator is only a heuristic and cannot be relabelled stabilization. This is a design requirement, not an implemented new runtime gate: the unchanged native runtime still reports its original global2% comparison. No supported earlier trigger has been established here; arbitrary multiplier/RMS/percentile/board-specific scheduling is not introduced.

Accordingly no production arithmetic/selector/mesh/default/API change was made, no expensive unchanged native checks/build were repeated, and zero of the at-most-two600s requests were consumed. No candidate performance or memory improvement is claimed. Manufactured early-stop acceptance, candidate addon and board runs are **not completed**, because no justified production candidate was selected. A new design decision is required if an earlier *unconverged heuristic proposal* to independent control is intended despite observed pointwise failures; this report does not treat such a proposal as accepted convergence.

## Evidence

`debugging/thermal-verification/stopping-policy/` contains the runnable new audit, all21 stage rows including original scalar witnesses/coverage, failed-negative fixture, new criterion witnesses and negative results, numeric final difference/utilization maps and measured stage plots, capture script and SHA256 manifest. Maps use actual saved arrays; no intermediate full field was invented. The original extended packet and failed600s runs remain in place. Heavy slot was released to P7; no physical process remains. No commits. STOPPED with blocker, pending parent review.
