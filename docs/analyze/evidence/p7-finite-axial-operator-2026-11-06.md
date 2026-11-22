# Finite axial magnetic operator foundation

The private `axial_magnetic` module eliminates both axial quadrature coordinates of the Newton interaction between parallel finite triangular extrusions. Finite endpoints and cross-segment interactions remain; it does not substitute an infinite-line model. No production dispatcher selects this operator yet.

For transverse separation rho, the double axial integral uses four endpoint values of `s asinh(s/rho) - hypot(rho,s)`. A shifted stable expression removes the common constant. Nonfinite inputs, invalid intervals, singular raw filament separation and unresolved endpoint cancellation refuse explicitly. This is not clipping or deletion of a physical return path; production integration still needs a supported fallback.

Disjoint sections support P1 nodal densities using two area quadratures. For constant profiles, a signed source-triangle fan integrates its radius analytically as well. The self/cross rule consequently retains only target-area and source-edge quadratures, and handles the logarithmic self singularity without sampling the raw zero-separation filament kernel. Zero-area fans contribute zero. Overlapping P1 profiles remain unsupported.

## Independent controls

Two signals and one physical return are represented by three extrusion cells. Balanced current modes are (1,0,-1) and (0,1,-1); the same return cell participates in both columns. Comparison is with the existing prism volume integral, whose source-z/radial integration is already analytic. Therefore merely counting eliminated coordinates is not a runtime comparison against that reference.

At quadrature orders 12/20/32, self-integral differences against the order-32 prism reference are approximately 1.4984e-4 / 3.1175e-5 / 4.0315e-6. At order 32, the largest Gram-entry and tested energy differences are approximately 4.04e-6 / 5.18e-6; control thresholds are 1e-5. These are convergence estimates, not rigorous enclosures or a physical case-matrix acceptance. The comparison is between distinct quadrature formulas on the same geometry.

Controls also check directional reciprocity, positive definiteness of the balanced two-mode energy matrix, translated geometry with unequal axial cuts, invalid endpoints, and 2/4/8-block axial partition invariance. Summing every cross-block interaction recovers the whole-interval integral; assigning independent returns to those blocks would not preserve this result.

A nonconstant P1 disjoint-section control additionally compares with an explicit six-dimensional volume quadrature at orders six and eight, using a different triangle parametrization and analytic linear densities. Both refinements agree with the reduced rule within 1e-7 relative. Shared-return reciprocity is checked at 1e-8 relative; its positive-definiteness check covers all balanced linear combinations of the two signals, not only three selected drives.

Debug timing for one order-32 self call is about 22 ms, versus about 92/398/1659 ms for 4/16/64 calls after explicit 2/4/8-way axial partitioning. This compares the same experimental algorithm at different block counts, not the existing STM32 solver. No channel-level speedup is claimed. Log: `debugging/p7-axial-operator-control.log`.

Final focused run: two tests pass in 4.33 s, including the additional P1/energy checks. Release addon build passes (`debugging/p7-axial-operator-native-build.log`). No full-board rerun is used to validate a private operator that has not yet been dispatched.

## Production obligations

The next integration must project physical current modes onto the uniform support, retain cross interactions with nonuniform/local 3D regions, and handle incompatible frames, arbitrary P1 overlap, extreme aspect ratios and cancellation with a checked fallback. Budget/deadline admission and independent error certification are still needed before dispatch. Existing full-volume paths and every case-matrix family, including remote returns and vias, remain required. Via representation is unchanged.
