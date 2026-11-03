# Physical terminal / infinity charge verification

Approved scope: [task and review conditions](../archive/tasks/next-task-electrical-physical-terminals.md).
Implementation: existing coupled conductor composer, descriptor frequency/time
backend and strict public/native DTOs. CPU/F64; placement F32/GPU unchanged.
This is a bounded P6 package, not closure of P6/P8/P11.

`portTerminals` declares actual positive/negative conductors; existing connect
welds both or explicitly selects one contact. Full C/A/B undergo congruence;
R/L retain independent blocks. **Absent intermodel mutual C/L is not extracted**:
close coupled objects require one jointly extracted model. Physical delay joins,
unknown returns/general B/reduced traces and shorted external ports reject.

Independent references are in `native/analyze/tests/physical_terminals.rs` and
`tests/helpers/electrical-physical-audit.ts`. Both-contact C is
`[[10,-5],[-5,10]] nF`, with R=50 Ω/L=.5 µH equivalent branch and Cdiff=7.5 nF.
The single-contact reference is a directly handwritten five-state ODE with
the three-node C `[[10,-2,-3],[-2,3,0],[-3,0,7]] nF`; independent 12-term
Taylor propagation is checked at .25/.125 ns. No production solver/composer
is used for references. An independently authored single original model is
also compared. Initial Q=15 nC,U=15 nJ; common-only Q=20 nC,U=20 nJ remains
nonzero and constant. Mathematical C-null gauges are tested separately.

Native coverage includes explicit ground return versus single contact,
permutation/orientation, continuation identity/seed, ambiguous charge, missing
trace/return, B mismatch, incompatible weld/steady/DC, permissions, domains,
memory and source deadlines. Public standalone DSL exercises both/single
contact, refinement, common charge, complex frequency and six refusals.
Installed package uses compiled worker and ordinary GPU-default shared addon.

Acceptance is fixed in the task: base/reference .25 mV, refinement .125 mV
and twofold improvement, charge drift <=1e-18 C for these coupons, normalized
energy balance <=1e-4 AND legacy absolute bound `1e-14+1e-7*norm`, with
nonnegative dissipation and separate raw balance/Dbe. There is no signed-history
correction. The strict energy gate can reject a tiny late continuation exchange
on a large charged rail when F64 cannot resolve its storage difference;
this is an explicit `COUPLED_ENERGY` refusal, not neutralization or a relaxed
energy floor. Earlier resolved continuation and identity negatives pass.

Generated evidence lives under ignored `debugging/p6-physical-verification/`;
previous evidence remains intact. Reproducibility:

```powershell
$env:CARGO_BUILD_JOBS='1'
$env:CARGO_TARGET_DIR=(Join-Path (Get-Location) 'debugging/p6-target')
cargo test --manifest-path native/analyze/Cargo.toml --test physical_terminals -j1 -- --test-threads=1
npm run native:build
npm run typecheck
npm run build
node --import tsx tests/helpers/electrical-core-verification.ts --physical
node tests/helpers/electrical-core-package.mjs --physical
node --import tsx --test --test-concurrency=1 tests/electrical-physical.test.ts tests/transient-adapter.test.ts
python tests/helpers/electrical-physical-render.py debugging/p6-physical-verification/direct-dsl/standalone/result.json
```

## Physical-terminal baseline (superseded by R1 below)

| Gate | Result | Evidence under `debugging/p6-physical-verification/` |
| --- | --- | --- |
| Physical native, including strict output axis/legacy continuation | 8/8 PASS,40.68s | `native-final-unique-axis.log` |
| Touched coupled regression | 9/9 PASS,18.72s | same log |
| Mixed/delay regression after physical routing changes | 13/13 and4/4 PASS | `native-final.log` (its superseded physical failure is explained in journal) |
| Shared source memory/deadline admission | 2/2 PASS | `native-admission.log` |
| Final ordinary GPU-default addon | PASS,7m28 | `native-build-final.log` |
| Typecheck/build | PASS | `typecheck-final.log`, `build-final.log` |
| Direct DSL / installed compiled worker | each8 calculations +6 refusals, independent audit PASS | `direct-dsl/standalone/{result,audit}.json`, `installed/{audit,manifest}.json` |
| Public adapter / saved corruption audit / source stimulus regression | 4/4 PASS,0 skipped;11 corruptions rejected | `ts-final.log` |
| Timeout, pre-abort, active abort, recovery / numeric contract | PASS / f32-rte-ftz-v1 | `installed.log`, installed manifest |
| Static PNG generation and actual visual inspection | 2/2 inspected | `electrical-physical-{contacts,charge-energy}.png` beside direct result |

Maximum node error106.490 µV; refined53.194 µV. Charge drift8.934e-23 C;
maximum normalized energy defect2.118e-14. Both-contact raw balance/Dbe
1.48878/1.48878 pJ refines to.743663/.743663 pJ; single-contact
1.06012/1.06012 pJ refines to.529640/.529640 pJ. Common-only Q=20 nC,U=20 nJ
has zero work/loss/numerical energy; it is not a gauge. Exact full/source and
artifact hashes, memory/RSS and timing are in `manifest.json`/`metrics.json`.
Maximum native plan566,960 B; measured worker high-water RSS91,996,160 B
(87.73 MiB, including Node/earlier calculations). Direct scenario wall1.39s.

The initial addon and duplicate-endpoint result are preserved as superseded
evidence; the final runner passed on untouched newly generated501-sample
outputs. The existing electrical output-count helper is reused. Conductor
trace allocation is limited to mapped/physical-initial requests so legacy
port-only output does not acquire an unnecessary per-node memory cost.

Heavy slot was coordinated with Thermal/P7 and released after final addon.
No PCB solves, commits, main changes or other worktree changes were made.
Await parent review; remaining classes are listed in the task.

## R1 — legacy energy guard retained, ready for review

[Review805f183](review-physical-energy-guard.md) is addressed solely in the
coupled energy predicate. Acceptance requires **both**
`abs(balance)<=1e-14+1e-7*norm` and `abs(balance)<=1e-4*norm`
(zero norm:1e-20 J). Dissipated/numerical energy must be nonnegative; norm uses
those nonnegative terms. No floor increase or history correction is introduced.
The focused predicate test rejects the conceptual norm1e-3 J/balance1e-8 J
counterexample; this does not claim an observed physical solver error.

New evidence: `debugging/p6-physical-verification/r1/`:

- Predicate1/1, physical8/8 (41.48s), coupled9/9 (18.34s) PASS.
- Latest ordinary GPU-default addon PASS,7m19; SHA256
  `4bd1d46c01c7b092548d6bb7b483bf8fb5415a3e08270d49e067401a90e3e267`.
- Direct and installed compiled-worker: each8 calculations+6 refusals;
  independent audit checks both guards and actual norm. Installed addon hash
  matches; f32-rte-ftz-v1 and timeout/pre-abort/active-abort/recovery PASS.
- Latest-result TS4/4,0 skipped,11 corruptions; typecheck/diff-check PASS.
  TS production/build output is unchanged; previous successful build retained.
- `numeric-equality.json` proves all numerical arrays/quality exactly equal
  before/after R1; previous two inspected PNGs remain applicable and are reused.
  Independent reference formulations/fixtures and PCB corpus were not changed.

Old evidence, package/archive/addon and a verbatim `r1/previous-manifest.json`
are retained. Root `manifest.json` points to latest R1 hashes and gates.
Heavy released to Thermal; no commits or changes to main/other worktrees.
