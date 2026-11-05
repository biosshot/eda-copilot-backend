# Adaptive magnetic operator evidence, 2026-11-05

The adaptive common-batch operator chooses Taylor degrees 0–12 on strictly
disjoint support balls, with positive absolute action goals and outward error
enclosures. It retains original tetrahedron current coordinates and the
existing numerical near/self kernel. A current-basis revision rebuilds all
magnetic images. Fresh recovered real and imaginary currents share a separate
cover for the original-row KVL certificate. The full KCL/KVL and power gates
remain at `1e-8`; physical and near/self quadrature accuracy remain unverified.

Representation version 3 and `certified-adaptive-harmonic-v1` bind cache identity.
Reports retain common-basis and per-column certificates. Supplemental frequency
batches compare original geometry and algorithm identities while retaining
their distinct certified numerical covers. Empty projection-current bases are
valid; physical output columns still require two-RHS certificates.

The focused release library compile passed in 3m19s after an action-goal
visibility fix and two borrow/capacity fixes. The broader integration check
encountered the pre-existing missing fixture
`docs/experimental/analyze/routing-board-v2-example.json`; that check is incomplete.
Eight focused TS checks, typecheck and package build passed after integration.

The prebuilt tiny proof executable passed clean strict supervision. Its saved
verifier independently compares original-coordinate actions with bounded
reference actions and checks manufactured original-current KVL rows.

| Tiny proof quantity | Result |
| --- | ---: |
| Supervised wall time | 0.218 s |
| Peak private commit / RSS | 3,616,768 / 14,782,464 B |
| Exit / supervisor stop reason | 0 / null |
| Original basis cases / manufactured KVL controls | 9 / 3 |
| Maximum action difference | 3.469446951953614e-18 |
| Maximum action error bound | 2.508497738520138e-15 |
| Maximum manufactured KVL upper ratio | 6.695985836813701e-14 |

Raw proof records are under `debugging/p7-adaptive-proof-20261105/`, including
`verifier.json`. They establish bounded operator and guard checks on the tiny
control. Full-board physical accuracy remains unverified.

The STM32 benchmark retains the saved original electrodes and two physical
matched DC source modes at the authored 10 kHz. DC source generation must pass
its original conservation and power checks before a shared magnetic action.
It measures an operator action on DC-derived currents; full AC channel response
remains unverified.

The first launch was wrapper-aborted during geometry after a CRLF/LF hash
assertion failed to stop a PowerShell sequence. Its `abort.json` is retained
under `debugging/p7-adaptive-stm32-action-20261105/`. The corrected wrapper
hard-gates byte hashes and normalized source equivalence before launch.

The first valid launch stopped at DC source generation before any magnetic
action or field solve. Records are under
`debugging/p7-adaptive-stm32-action-valid-20261105/`.

| First valid STM32 attempt | Result |
| --- | ---: |
| Supervised wall time | 95.078 s |
| Peak private commit / RSS | 594,571,264 / 479,571,968 B |
| Exit / supervisor stop reason | 101 / null |
| Drive-0 DC KCL / constitutive KVL | 5.747505988968134e-5 / 2.1713324703337227e-12 |
| Source/loss power / matched-wave power error | 3.300058325947852e-4 / 6.606187733698457e-4 |
| Magnetic actions / field solves | 0 / 0 |

The benchmark-only retry allows up to eight residual corrections with the same
DC factor. Both physical source columns pass the unchanged gates after two
corrections each. Each column has 90,003 active original copper cells.

| Final source check | Drive 0 | Drive 1 |
| --- | ---: | ---: |
| Original KCL relative | 6.268850383707411e-12 | 7.589796219045614e-12 |
| Constitutive KVL relative | 2.2246338667643067e-17 | 4.44926773318285e-17 |
| Source/loss power relative | 3.601514816158551e-11 | 3.6013089162456547e-11 |
| Matched-wave power relative | 7.204897629897733e-11 | 7.204499934132525e-11 |

The subsequent shared two-column adaptive magnetic batch starts but exceeds
the outer process-tree deadline. Final records are under
`debugging/p7-adaptive-stm32-action-refined-20261105/`: `outcome.json`,
`supervision.json`, `run.log`, `identity.json` and
`physical-source-controls.json`.

| Final STM32 attempt | Result |
| --- | ---: |
| Supervised wall time | 1,800.047 s |
| Exit / supervisor reason | 1 / outer process-tree deadline exceeded |
| Peak native subtree private commit / RSS | 1,328,381,952 / 734,289,920 B |
| Original copper tetrahedra / potential / current coordinates | 154,899 / 65,206 / 464,697 |
| Completed geometry / mixed ownership phases | 82.315260 / 1.848034 s |
| Completed DC factor, source and checks / magnetic preparation | 0.695972 / 0.715992 s |
| Completed magnetic action / returned error certificate / comparison | false / false / false |
| Field solves | 0 |

The measured memory scope is the native test executable and its descendants;
the Python supervisor is outside that subtree. The observed private peak stays
within the 4,096 MiB tree cap during this incomplete attempt. Completed-action
peak memory remains unverified. Native memory admission remains 3,072 MiB.

No separately completed action duration or partial cover counts are available.
Cleanup confirms the benchmark and supervisor processes have stopped. The full
STM32 operator time requirement is unmet. Completed-action memory and accuracy,
full AC response and P7 closure remain unverified. No further numerical run is
scheduled in this batch.

The final native addon build passes (5m16s). No numerical rerun followed it.
