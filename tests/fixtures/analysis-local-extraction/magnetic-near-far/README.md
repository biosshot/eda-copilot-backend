# Bounded near/far magnetic diagnostic
The task, source and fixed policies are frozen before measurements. Two rigid
copies of the accepted 12-prism, two-streamfunction closed foil are separated
by 3 mm in X; each has its own two physical basis currents (24 supports/4 DOF).
No source currents, prism extents or weak coefficients are discarded.

Run driver modes build, plan, coupon, reference in an exclusively coordinated
heavy slot. Each numerical process has hard 512 MiB/60 s caps, a unique saved
run and no automatic completed reruns. Plan does not evaluate any kernel.
The coupon exact cache admits at most 64 supports. The independent reference
reuses frozen self blocks; only the smooth separated cross blocks are new.

Fixed theta controls 0.7/0.4/0.2, coupon leaf1/ROI leaf8. Full gate5%, analytic
far budget1%, raw reciprocity1e-9. The far truncation bound is conditional on
exact moments and near integrals; quadrature, F64 and empirical reference
uncertainty are separate. No clipping, per-pair relative acceptance, ROI solve
or public API admission. Far block storage is hierarchical; pair-streamed
application still costs O(N²), which is reported rather than hidden.
