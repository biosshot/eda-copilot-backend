# P7 full STM32 automatic attempt, 2026-11-05

The runner used the existing complete-channel DSL at
`tests/pcb-analyze/STM32G0_CAN_RS485/local-extraction-complete-channel.js`.
It retained its explicit TOP pad contacts, singleton 10 kHz request and
authored materials. The execution budget changed from 300 s / 2,048 MiB to
1,800 s / 3,072 MiB. The saved source and board hashes are in
`debugging/p7-stm32-automatic-20261105/identity.json`.

Geometry planning passed in 1.572 s and selected the automatic joint producer.
The candidate retained all 844 captured copper regions, 65 islands and
89 plated barrels. Its ROI was approximately 65.7918 × 45.7918 mm. The
isolated exact-grid cache was empty.

One supervised cold native attempt refused during common copper arrangement,
before electric or magnetic field preparation and before the conductive Schur
factorization. The unchanged area guard reported
`LOCAL_BOARD_ARRANGEMENT` for physical copper set 27. Relative area change was
6.90153493215147e-8 against the 1e-8 limit. The original and reconstructed
sets each had one component and one hole.

Set 27 maps to plated hole `hole:via:d01d9148439d1f51`, source
`copper.fixed.vias[2]`, on RS485_B. The source through via is at
(19.8617, 6.2463) mm, with 0.5994 mm copper diameter and 0.2997 mm finished
drill. Its captured bore has 28 vertices. This mapping follows the two copper
layer sets and native plated-hole order and agrees with the expected barrel
list. The exact failure in Boolean reconstruction remains under diagnosis.

| Supervised attempt | Result |
| --- | ---: |
| Wall time | 8.890 s |
| Peak tree private / RSS, 20 ms sampling | 302,518,272 B / 266,579,968 B |
| Native budget | 1,800 s / 3,072 MiB |
| Supervisor limit | 1,860 s / 4,096 MiB private commit |
| Host reserve / threads | 1 GiB / 1 |
| Native retry | None |

The original source states constant bulk copper at 25 C and lossless dielectric
with relative permittivity 4.2. Measured stackup, dielectric loss, roughness and
channel response remain unavailable. Captured CAD fill freshness remains
unknown. This attempt returned no extracted model, so full-channel physical
accuracy and resource admission remain open.

A separate planning-only attempt on the manifest fixture at
`tests/fixtures/analysis-p7-closure/STM32G0_CAN_RS485.js` refused with
`LOCAL_INPUT: Contact layer required`. That fixture requests 1–10 kHz and
omits explicit contact layers. Its saved refusal is under
`manifest-preflight/` in the current evidence directory and involves zero
numerical solves.

The direct DSL attempt is saved under
`debugging/p7-stm32-automatic-20261105/`, including original/executed source,
the original board, automatic request, source ownership, result and supervisor
report. The failed extraction did not proceed to warm or restart replay.
