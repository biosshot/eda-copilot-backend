# Thermal mesh production boundary

## Approved opt-in policy (2026-10-23)

The low-level recipe below was rejected as public API. It remains an internal
regression fixture only. Parent instead approved implementing exactly:

```js
mesh({maxEdgeMm:1, geometryDeviationMm:.005, maxNodes:140000,
  refinement:{maxPasses:5, relativeTolerance:.02, validationMaxEdgeMm:.5}})
budget({maxSeconds:900, maxMemoryMiB:1024})
```

Policy is optional; ordinary defaults/behavior stay unchanged. Passes are bounded
1..6; relativeTolerance is 1e-4.. .02; independent validation spacing is positive
and strictly smaller than the seed maxEdgeMm. The controller's explicit
140000-node/600000-link ceilings, 1024 MiB, total native 900 s and outer scenario
1200000 ms are authorized. They are not increased defaults.

The implementation uses the verified fixed physical source halo/graded seed,
directional midpoint marking and retention of every prior triangle edge. The
first successive candidate field pass triggers one independent nested global
control; it does not start an unbounded accuracy search. Final published full
arrays belong to that control, never the preceding candidate. Intermediate
passes retain scalar history and only the mesh/coordinate data needed by the
next pass/comparison. Retained data is included in simultaneous memory admission.

At every shared output/event time t, define d(t)=max sampled |Tfine-Tcoarse| and
r(t)=max sampled |Tfine-ambient(t)| over physical layers. Compare native common-
origin coordinates, avoiding premature absolute-temperature reconstruction.
The gate is d(t)<=relativeTolerance*r(t)+1e-9 K at **every** time; the 1e-9 K
allowance is separately reported for zero/near-zero signal. There is no controller
temperature scale or use of absolute Celsius as the relative denominator.
Sampling includes both physical nodal sets, authoritative boundary vertices /
midpoints and inward traces; missing physical/boundary coverage fails the gate.
This is sampled convergence, not certified continuous error or absolute accuracy.

Transient refinements rerun the same physical history and initial condition at
every level and compare all output times plus required power/ambient/cooling
events. A steady gate cannot certify a transient. Continuation is not remapped
onto a new mesh: the opt-in refinement path rejects an incompatible continuation.
Original nodal <=1e-8, energy and time-integration guards are unchanged.

One monotonic start/deadline, memory and output budget cover all mesh preparation,
passes, independent control, comparison and final report. No per-pass deadline
reset or post-timeout processing is allowed. Resource/nonconvergence can expose
only an already computed numerically valid field with explicit unconverged
conditional provenance, or a normal unavailable/refusal; physical/numeric failure
cannot become a valid map. Detailed intermediate arrays are not mandatory public
results. Direct DSL files may be separated by case under the unchanged 128 MiB
scenario output cap. Large controller execution requires a small adaptive
regression and actual admission evidence first.

The small analytical/event/deadline gates passed; one controller steady
measurement is approved after the shared build queue. Future controller
transient files may explicitly author reportStep30, giving observations at
`[0,5,20,30,60]` s after mandatory event insertion. This changes reporting only,
not the physical power/ambient history or internal time integration. Graphs and
checks describe sampled values; these five observations cannot establish a
continuous peak or complete transient shape. No new reporting API or automatic
numeric/output-budget increase is part of this policy.

## Rejected replay alternative and report measurements

Parent hashes and numerical edge IDs are deliberately **not** public DSL fields.
Their saved replay plans remain private regression fixtures. The former proposal
was rejected because it exposed internal numbering and required near-complete
prior analysis; it must not reappear through a generic mesh/cache resource.

Saved old native candidate/control results are55772790/70046940 bytes. A minimal
projection with currently enumerable public finalState and temperatureMap totals
138145600 bytes, above the unchanged128MiB cap134217728, before actual metadata
or plots. These measurements explain separate case files and retaining only one
final full result inside the opt-in run. They do not justify dropping final
computed arrays or raising the output limit. The new final coordinateFrame and
scalar history must also be included in actual output admission/measurement.

Evidence is in ignored controller-refinement-report-volume.jsonl,
controller-final-pair-report-lower-bound.json and controller-nested-global-half-
comparison.json; previous diagnostic failures are preserved. Parent review and
three-board/addon/package acceptance remain pending.
