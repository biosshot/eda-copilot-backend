# Balanced multiport DC response

The existing exact DC endpoint now requests a balanced external-port response
in addition to its primary loop probe. Each port is an oriented edge between
the loaded physical islands. A spanning forest supplies a deterministic basis
of balanced cycle drives; ports wholly within one island have independent unit
drives. Resistive package stamps affect actual connectivity, while capacitors
remain open. No common reference conductor is invented.

One loaded conductance factorization serves the primary and all modal drives.
The output reports the drive basis B, the gauge-invariant modal impedance
B^T V, original individual port voltages where defined and null otherwise,
and full electrode residuals. Independent unit-port drives are declared only
when all external ports support them. Private package ports are excluded from
the excitation basis.

Each mode also checks terminal input power against copper Schur dissipation,
external termination loss and private package loss. These are terminal-level
losses, not recovered modal cell currents. The original primary cell recovery
and loss reporting remain separate. Review corrected the initial label that
would have included direct-native external loads in package loss.

The spanning-forest construction passed 2048 exact graph checks. The focused
isolated Rust algebra test passed, including disconnected/open ports, a
balanced cycle, reciprocal cross terms, changed island gauge anchors and a
resistive bridge. The final split-loss case gives 4/9 W in an external 4-ohm
load and 8/9 W in a private 2-ohm package, with 4/3 W total at 1 A.

An early invocation against the previous addon correctly refused the new
request field before solving. The integrated native build passed in 10m12s. Addon SHA-256: `5967213e65a6b535112c9e6aa57d89550898fec3ae2fabda4e384753e59d919b`.
The exact integrated release algebra unit also passed. The two DC test files
passed all four tests in 2.409 s, including actual-addon
assertions on the new modal response. Independent
spatial accuracy, PDN board acceptance and DC/AC overlap remain unverified.
