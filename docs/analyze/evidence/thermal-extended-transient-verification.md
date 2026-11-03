# Extended TPS transient: complete fields, temporal gate FAIL

The sole authorised TPS63060DSCR tighter request completed under the diagnostic
1800s shared deadline. Full-field temporal change is **0.1168551214%**, exceeding
the unchanged **0.1%** gate. Spatial convergence and original physical guards
pass. This does not establish temporal convergence of this scenario, whole P9
acceptance, or compliance with the600s performance target. No retry followed.

## Exactly one compatible request

Task `next-task-thermal-extended-convergence.md` (MAIN cac5f8b) explicitly allows
one extended convergence diagnostic. `thermal-extended-time-refined.js` differs
from the frozen tighter script only in explicit `maxSeconds:600`→`1800` and its
matching comment. The `.run()` count is1. There were zero ordinary, Band,
controller, curve or coupon reruns and no algorithm/cache changes. Its seven
internal physical analyses are the unchanged seed/five-directional/independent
control policy within that one shared request.

Native maxMemoryMiB384, maxNodes50000,2M numeric numbers,128MiB result,
geometry deviation.001mm, full60s physical history and numerics2.5e-5/2.5e-6
are unchanged. Original source5s/ambient20s events and observations
[0,5,15,20,30,45,60] remain. Launcher2100s supplies transport/error-save headroom;
1800s is an explicitly approved diagnostic ceiling, not a production default/SLA.

The original verified addon
`6b86525c1372b1472a2e4643b97c16f1f75db1dc192d031387e8fdf4135447dc`
was recovered from the immutable native-boundary snapshot and matching installed
package. A local isolated copy contains the exact original compiled runtime;
all45 owned runtime files were hashed,25 Analysis source hashes match baseline
provenance, and compiled dist bytes match the accepted baseline worktree.
Original installed dependency lock/workerpool10.0.3 and package self-resolution
were checked without invoking any numerical method. The main addon remains
byte-identical to the pre-extended checkpoint. Test-only profiling/supernodal
prototypes are absent from this original production binary.

Saved ordinary run: `2026-10-24T10-53-36-958Z-thermal-final-time-zi5hXc`;
full file SHA256 `2edb1872a409e74d567223d0c0f1e47b4c32dacb03d1befc92b2f51d6196ed93`.
Its board hash, authored physics, source/contact identities, schema, numerics
and prior spatial1.546715944%/missing0 evidence were checked before launch.
Measured ordinary sizing was1,899,440 numeric numbers,50,454,326B result and
338,097,846B maximum native plan. These measurements were a preflight reference,
not permission to raise caps for a different adapted mesh.

## Measured result and guards

Extended run: `2026-10-24T15-08-11-566Z-thermal-extended-time-refined-BXgR2Q`;
full file SHA256 `4ccdf37f294fbff569bee2e53b71028ac2ffe45d55652150f2342fa0fcfee72e`.

| Measurement | Result |
| --- | ---: |
| Shared seven-analysis native study |1018.935835s /1800s|
| Public scenario wall |1022.912069s|
| Launcher observed wall |1028.740160s /2100s|
| Hard kill / retries |none /0|
| Final nodes / links |42660 /173833|
| Numeric numbers / result bytes |1899440 /50454015|
| Maximum native planned bytes |338097846 /402653184|
| Native-reported process high-water RSS |535781376B|
| Sampled worker peak RSS / private |442572800B /394416128B|
| Sampled launcher peak RSS / private |513187840B /522809344B|
| Independent-control spatial change |1.546711450% /2%|
| Original nodal / linear residual |5.405655410e-12 /6.265717889e-12|
| Normalized energy balance |3.535466973e-13 /1e-6|
| Maximum accepted adaptive local error |0.959739928 /1|

The384MiB API limit is the unchanged conservative allocation admission, not a
new OS RSS cap. The native RSS metric includes Node/prior calculations; the
five-second external sampler can miss short final allocation peaks. These
distinct metrics are retained as reported, without substituting sampled RSS
for the native process high-water value. No memory/output guard was disabled.

| Stage | Role | Wall s | Accepted / rejected | CG iterations | Factors |
| --- | --- | ---: | ---: | ---: | ---: |
|0|seed|79.250|1483 /12|19550|28|
|1|directional|112.868|1518 /12|19962|27|
|2|directional|141.708|1529 /11|20131|29|
|3|directional|154.895|1528 /12|20112|29|
|4|directional|154.897|1532 /12|20101|32|
|5|directional|167.856|1530 /12|20111|30|
|6|independent-control|206.165|1534 /11|20305|29|

Totals are10654 accepted/82 rejected steps,140272 CG iterations and204 factors.
Stage wall values include whole analyses and are not factor/CG subphase times.
The original per-stage physical/adaptive guards and shared memory admission pass.

## Full-field temporal comparison

The accepted physical-domain P1 helper checks both nodal sets and CAD physical
boundaries/traces, plus compact package nodes. Final mesh points/triangles are
identical, layer/source/contact coverage agrees, and missing physical/boundary
samples are0 at every time. Thus different final spatial meshes do not explain
the measured temporal difference. Full arrays remain preserved.

The unchanged normalization is
`abs(T_tighter−T_ordinary) / max(1K, abs(T_tighter−25C))`.

| Witness | Relative maximum | Absolute maximum |
| --- | --- | --- |
| Location/time |core,(-0.5,-13)mm,20s|U1:junction,60s|
| Ordinary / tighter |25.884556746 /25.883388195C|77.651901506 /77.662871284C|
| Signed difference |−0.001168551214K|+0.010969778010K|
| Denominator |1K|52.662871284K|
| Relative change |0.1168551214%|0.0208301935%|

The junction-only relative change would pass0.1%; the core full-field witness
does not. The current ambient is30C at20s, but the approved normalization uses
the initial25C ambient and1K floor. That denominator was not changed to obtain
a passing result.

Saved negative substitutions:26/26 rejected, original data restored and re-audited.
Numeric overlays, signed spatial differences and complete temperature-time
difference maps cover TOP/core/BOTTOM and all seven times. Both PNGs were viewed;
the maps are actual accepted P1 samples, without fabricated intermediate times.
The map capture contains28543 physical points and14,611,906B of saved comparison
data; compact nodes are separately included in the acceptance witness search.

## Preserved package and stop

Evidence: `debugging/thermal-verification/extended-convergence/evidence.json`.
It links preflight/runtime hashes, the sole request guard, complete outputs,
original600s failure, unchanged ordinary data, original guards/negative tests,
OS observations, source snapshots and viewed numeric PNGs. Earlier failed runs
and rejected optimisation evidence remain untouched. Heavy slot released to P7.

The one authorised request is exhausted. Temporal acceptance remains FAIL;
whole P9 remains open. No production changes, commit or further runs were made.
