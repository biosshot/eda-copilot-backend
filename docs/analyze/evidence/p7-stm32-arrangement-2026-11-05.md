# P7 full STM32 arrangement geometry, 2026-11-05

The final common arrangement now uses the existing maximum-range checked
integer adapter for rectangular domains as well as polygonal domains. Its
bounds include every material, contact and partial-bore input. The adapter
preserves inherited dyadic vertices and checks the pinned integer range.
Common partitioning preserves an atom unchanged when strict bounding boxes
are disjoint, when the cut covers the complete atom, or when their interiors
have no intersection. The latter predicates use the complete F64 DE-9IM
relation, including holes. Partial overlaps continue through Boolean splitting.

The original 1e-8 copper and finite-contact area limits, component and hole
checks remain in force. The patch does not round source inputs or restore an
area scalar.

The focused source-location annulus test passed in 0.093 s. A separate ignored
native harness loaded the exact saved solver board and geometry from the
[previous automatic attempt](p7-stm32-automatic-2026-11-05.md). It used the
same production allocation-budget calculation and output reserve, and built
the mesh once with zero field solves.

| Full saved STM32 geometry preparation | Result |
| --- | ---: |
| Supervised wall time | 82.890 s |
| Original source mappings / plated barrels | 844 / 89 |
| Common arrangement atoms / nodes | 975 / 113,172 |
| Copper / dielectric tetrahedra | 154,899 / 410,871 |
| Maximum relative copper area change | 7.57514e-10 |
| Maximum relative finite-contact area change | 2.31280e-9 |
| Planned mesh bytes | 582,969,984 B |
| Peak tree private / RSS, 20 ms sampling | 535,273,472 B / 480,636,928 B |

All expected source mappings, barrel IDs and original finite contacts were
checked. Both geometry controls passed under one thread, an explicit 1,800 s
native deadline and 3,072 MiB native allocation cap. Full preparation used
the existing 1,860 s supervisor with a 4,096 MiB tree private commit cap and
1 GiB host reserve.

Evidence and the ignored geometry-only harness are under
`debugging/p7-stm32-arrangement-20261105/`. The original full extraction
refusal remains in its separate evidence directory. No field solve or full
extraction retry was performed after this change. Electrical and physical
accuracy, and admission of the remaining operator allocations, remain open.

The native addon build passed in 5m 38s. SHA-256
`e7a4657de6ef2070c2a589ef8047fcf12bcf736db8f7812b503b41b7143d42b4`.
