# P7 complete-channel continuation — 2026-10-30

Status: **incomplete; no complete-channel5% certificate**. This report does not close the approved `next-task-p7-complete-channel.md` assignment.

## Remote physical request

`tests/pcb-analyze/STM32G0_CAN_RS485/local-extraction-complete-channel.js` uses the existing DSL without scope, positive U5.7 → J5.2 and separate return U5.5 → J5.3. Unlike the earlier `local-extraction-automatic-return.js`, it does not place both signal terminals on opposite faces of the same via. Existing fixtures and scenarios remain unchanged.

Run `node --import tsx scripts/inspect-p7-complete-channel.ts` for **geometry planning only**, then `python scripts/render-p7-complete-channel.py`. The callback intentionally stops before physical execution; its successful status is not a successful field solve. Saved `debugging/p7-channel-plan/remote-plan-1793350766051/evidence.json` records214 connected signal/return regions. The source-bound audit lists every region's source path, layer, physical island, role, numerical candidate inclusion and reason. Outside-candidate connected copper is explicitly a truncation to verify, not silently admitted as included copper.

A focused regression confirms remote endpoints, both real branches D5.2 and JP2.2, BOTTOM signal copper and unique per-region audit coverage. The rendered actual track/via/terminal PNG was opened and inspected; it shows the layer changes and branch, with an explicit planning-only label. This is not a segmentation visualization: no longitudinal electrical partition has been produced.

## Explicit developer resources

`python scripts/verify-p7-automatic-supervised.py --native-memory-mib 3100 --tree-memory-mib 3200 --reserve-mib 256 --seconds 1800`

The command retains the original four-terminal local via candidate. It is not the remote-channel acceptance above. `P7_VERIFY_MEMORY_MIB` and `P7_VERIFY_SECONDS` explicitly enable bounded developer verification through DSL/native preparation/response checks. Defaults remain2048MiB/300s, native geometry count caps and physical criteria remain unchanged. Developer upper limits are8192MiB/3600s; response solves keep their existing30s per-call cap inside the shared lifetime. No production band is widened. Four training jobs still share immutable factors.

Supervisor requires available RAM to cover the entire tree cap plus declared host reserve before spawning, samples tree RSS, checks available host RAM throughout, kills its own tree on a guard violation, and saves `resume-manifest.json` before/after execution. It does not claim per-point native journal resumability. Each attempt uses its own empty cache; no warmed run is labelled cold.

Preserved preliminary attempts:

- `automatic-supervised-1793350296386503800`:3456MiB tree +512MiB reserve refused before spawn,3,805,757,440bytes available.
- `automatic-supervised-1793350304227455900`:3100MiB native/3200MiB tree,1800s,256MiB reserve; old adapter refused `LOCAL_BUDGET` in2.051735s,226,463,744byte tree peak. This led to the limit plumbing change; no FEM solve was completed.
- `automatic-supervised-1793350539404468700`: intentional64GiB tree preflight negative refused without spawning.

## Exact remaining architecture

`channel-plan.ts` currently builds island envelopes and candidate domains, not ordered section/local ownership or interface terminals. `local-executor.ts` calls one volume extraction per candidate/patch level. It cannot currently partition long uniform tracks into transmission-line models or compose branch-coupled local junctions through P6. The remote request above therefore remains a planning regression, not a deliverable complete-channel electrical model.

`volume_current`/`volume_magnetic` use volume-current coordinates. The tangential surface-current trial space with continuity/contact constraints and its external magnetic operator required by `p7-hf-operator-design.md` is absent. The polygon section surface Gram matrix cannot be applied to a via by changing frequency or multiplying volume DC resistance. A genuine extension of the existing physical operators and verified volume/surface overlap is necessary; this continuation does not implement it.

Still missing: longitudinal segmentation and section/local composition with nonduplicated copper, joint returns/coupling, multiple/floating reference constraints, local3D HF/transition operators, continuous common-band causality/passivity verification, positive complete STM32/BandAmp/TPS transient cases and independent assembled≤5% observables. The previous1–100kHz conditional local models and section controls remain the only supported physical classes. No new P8/P11 readiness is claimed.

## Final measured attempt and checks

Ordinary native build PASS8m42s, SHA256 `7018b779c51c75006d83be5ab2951f436328c29e3b2cb68febd3f197e3c4345c`. An earlier successful5m08s build preceded the final response-limit plumbing and is not this measured binary. Final TypeScript typecheck/build PASS; focused adapter/cache/domain16/16 PASS. Native preparation3/3 passed before the final response helper visibility change; final focused preparation/response rerun is recorded separately below.

`automatic-supervised-1793351279783736600` is the **actual increased-memory cold attempt** on the final binary:3100MiB native,3200MiB tree,256MiB host reserve,1800s deadline,4 shared-factor training jobs. Available RAM at preflight4,627,140,608bytes. Coarse factor/training/preparation advanced and the fine321554-unknown magnetic system reached symbolic planning. At85.2317893s the host available-memory guard stopped the owned family; tree peak3,330,764,800bytes, largest child3,259,428,864bytes. No scenario result, fine factor completion, sensitivity or channel admission is claimed. The existing safe guard was not disabled. This is an interrupted cold attempt with retained coarse cache, **not** warm/restart evidence.

`automatic-supervised-1793351437402709300` records a4096MiB native/5120MiB tree/512MiB reserve preflight refusal at3,198,967,808bytes free (focused native recompilation was then active). That attempt never spawned. These measurements establish actual host pressure, not a new mathematical infeasibility or a permanent2GiB criterion. A future host with at least the complete tree+reserve available can reproduce the same bounded command. No steady-state timing or accuracy can be inferred from the interrupted fine run.

No new verified physical band or final observable error exists for the remote channel. Full-channel bands/errors/delay/coupling/loss/transient and admitted cold/warm/restart rows are **unavailable**, not zero. The existing conditional local1-100kHz and section results retain their previous limits. P8/P11 dependencies have not expanded.

Final-source focused native command `cargo test --release --manifest-path native/analyze/Cargo.toml --test board_prepare --test board_response -j1`: **10/10 PASS**, release compilation3m32s. No expensive field campaign was repeated by those tests. Code commit `c2e278c`; no push. User-owned `native/analyze/debugging/` was neither changed nor staged.
