# P7 bounded exact magnetic working set, 2026-11-05

The streamed original-tetrahedron magnetic action now retains a bounded cache.
It stores at most 64 entries of 128 KiB and reserves two additional entries for
construction and near/self scratch. It evicts before constructing a replacement
and pins the current target while loading the source. Original geometry costs
96 bytes per tetrahedron and is moved into the cache once.

Each entry covers 25,344 bytes of separated cubature at orders 4, 6 and 8,
32,000 bytes of lazy far-source samples, source geometry and metadata. The
existing separated-order rule selects those three orders. Near and self
integrals still receive the original maximum order. The dense reference
operator, pair order, coefficients and compensated accumulation are preserved.

The native mixed buffer planner now accounts for its actual four-term current
columns, sparse triplet construction and full-vector temporaries. On this
64-bit build those buffers reserve 1,376 bytes per current coordinate,
plus 1,024 bytes per original potential. It retains the conservative
64 r (n + m) mode allowance and 8,192 r² descriptor allowance, where
r includes the requested enrichment budget. Dynamic sparse factor fill,
all retained owner/trace factors and queried solve scratch still have their
existing pre-allocation guards. The mixed runtime uses the same buffer
formula when admitting the coupled system.

## Focused release verification

| Control | Supervised wall time | Result |
| --- | ---: | --- |
| Exact pair coefficients through eviction, near and self | 0.094 s | Bitwise equality with original evaluator |
| Request mode-cap, overflow and memory refusal | 0.078 s | Pass |
| Existing compact automatic endpoint vs. monolithic | 5.656 s | Pass |

The compact endpoint had a 3,694,592 B private tree peak and
11,235,328 B RSS peak under 20 ms sampling. Maximum complex S difference was
1.11022e-16. Full relative KCL and KVL residuals were 6.49131e-10 and
9.11010e-11. Column power discrepancy was at most 4.78046e-15 W. Each control
ran once under one thread, a 1,800 s supervisor deadline, 3,072 MiB native
ceiling and 4,096 MiB tree private commit cap with 1 GiB host reserve.
The focused kernel test additionally retains its internal 30 s cache deadline
and uses a 32 MiB arithmetic admission check.

For the saved STM32 count of 154,899 copper tetrahedra, the cache plan is
23,521,056 B (22.4314 MiB), including geometry, 64 entries and two scratch
entries. This is a count-based plan check. No full STM32 field solve was run.
It excludes common fields, original coordinate buffers, basis/action slabs
and factors. Native mixed scalar current buffers alone reserve 639,423,072 B
for the corresponding 464,697 coordinates.

Pair work remains quadratic. A nonempty action batch on that STM32 mesh still
visits 11,996,927,550 tetrahedron pairs. The working-set change establishes
bounded exact storage, with full-board runtime and physical acceptance open.

Logs are under `debugging/p7-streamed-memory-20261105/`, with separate
`cache/`, `plan/` and `compact/` supervision, comparison and compile logs.

The native addon build passed in 5m 44s. SHA-256
`da72efd7557a310308d190f3ea561ceb2ac6efed3164d6618aa2f6077eb63e4b`.
