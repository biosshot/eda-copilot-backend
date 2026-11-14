# Curve-tolerance comparison plan

This is a saved-geometry design checkpoint; no new controller .01 request or
curve acceptance is claimed. Use the retained `.005` controller report and any
retained `.01` geometry/report only after verifying board/source/stack/cooling
identities, geometry options and common coordinate origin. Geometry without a
complete matching temperature field cannot establish thermal convergence.

For each layer retain both complete prepared physical domains, material regions,
original CAD outline, holes/barrels and source/contact supports. Compute polygon
intersection, A-minus-B, B-minus-A and union in common physical mm coordinates
using the repository's geometry Boolean machinery. Save signed areas, absolute
areas, symmetric-difference fraction, connected components and source-region
overlaps separately. Boolean coordinate roundoff must enter the area uncertainty
bound; the authored curve deviation is a bound, not a measured displacement.
For original CAD arcs compare source identity and parameter intervals, curve
deviation bounds, transported edge correspondence and measured boundary distance
with an explicit geometric error bound. Hole/outer/material boundaries remain
distinct. Identical board identity alone does not prove matching material regions.

Compare both nodal sets, domain/material boundaries, contact supports and inward
traces on the intersection with strict P1 coverage and the accepted physical
ambient-rise normalization. Report missing samples by time/layer/source and by
interior/boundary; publish signed absolute and normalized witnesses. Keep compact
node/source/contact comparisons and original nodal/energy guards independent.
Proven Boolean outer-edge correspondence is permitted only for the same source
edge/fraction within its explicit displacement bound; it cannot bridge a
different curve approximation, hole, changed material or removed cell.

The intersection result is explicitly a **common-domain** comparison. For each
part of the symmetric difference report which field is undefined, its area and
boundary, changed material/capacity/conductance/source support, available one-sided
temperatures and a separate missing-coverage count. Never replace absent values
with zero or silently omit that part. Full-domain thermal curve acceptance stays
unestablished unless the domain difference is empty within proven Boolean
roundoff or a separately reviewed conservative physical error bound covers its
thermal effect. Small changed area alone is insufficient, especially near narrow
copper/contact paths. Mesh and time convergence must pass independently for both
curve geometries before interpreting their field difference.

Preflight must admit storage for both saved domains, clipping workspace and
bounded sample indices. Preserve both raw geometry/report hashes and all failed
coverage diagnostics. Reuse saved fields first; any missing controller field or
new larger solve requires separate budget/design review. No public API, model,
cache policy or acceptance threshold changes are part of this plan.
