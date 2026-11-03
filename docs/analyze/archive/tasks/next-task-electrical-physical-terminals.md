# P6: физические контакты при соединении infinity-reference моделей

2026-10-24. Mixed delay/RLC принят189279e. Следующий необходимый стык P6/P7 —
соединение электрических моделей с сохранением настоящих проводников и зарядов.
Это checkpoint проектирования; production-код — после review постановки.

## Результат задания

Предложить минимальное расширение существующего coupled-rlc Resource для
явного соответствия port -> positive/negative physical terminal. Показать один
TypeScript объект и native DTO, применение через существующий a.connect.
Не создавать новый DSL/универсальный connection framework.

Принципиальная граница: равенство двух дифференциальных напряжений и противоположные
port currents само по себе описывает идеальную связь портов, но не обязательно
электрическое соединение обоих физических контактов. Нельзя автоматически
объединять common modes, grounding infinity или выбрасывать внешнюю ёмкость.
Роли positive/negative должны быть однозначными и явными.

- Начальный класс: unreduced coupled модели с явно сохранёнными physical
  terminal potentials. Для general/noncanonical B и reduced basis описать,
  какая trace информация необходима; неоднозначное соответствие отвергать.
- Полная сборка должна сохранять R/L/C energy, Kirchhoff constraints, заряды
  и математические gauges. Различить zero-energy gauge и физический common charge.
- Предложить совместимость grounded/delay блоков и infinity моделей: где явно
  соединяется return, где нельзя соединять до дополнительных данных.
- P7 не должен дублировать time solver; external model sources не меняются.

## Обязательные независимые coupons в проекте

1. Две full-C модели с явным соединением двух контактов; сравнение с независимо
   собранной одной исходной схемой, включая nonzero common charge/energy.
2. Отличие соединения только одного контакта от обоих, отсутствие hidden ground.
3. Переименование/перестановка potential nodes и обращение port orientation.
4. Negatives: inconsistent B/terminal map, missing return, неизвестный reduced
   trace, неоднозначный initial charge и невозможная DC steady постановка.

Зафиксировать уравнения, знак currents, admission, сохранение domain/permissions,
конкретные numeric tolerances и оценку memory/time. Показать взаимодействие
с существующими coupled/composition и accepted mixed backend, не писать второй.
Записать план реализации/тестов в этот файл и краткую матрицу остатка P6.
Никаких builds/новых solves для документации. Не объявлять P6/P8/P11 закрытыми.
Не коммитить; сообщить готовый checkpoint и остановиться до review.

## Design checkpoint — 2026-10-24, на review

**Parent review 2026-10-24: одобрен для реализации с условиями ниже.**
Проверены recovery/congruence, signs, initial charge и оба analytically specified
coupons. Следующее поручение — реализовать и проверить описанный блок, не ещё
один design-only этап; production entry и приёмка заданы далее в документе.

- Block-diagonal assembly соединяет предоставленные модели, но не вычисляет
  отсутствующие межмодельные mutual C/L. Это допущение показать в semantics и
  отчёте; физически близкие объекты с существенной взаимной связью должны
  предоставляться совместно извлечённой моделью. Weld не заменяет EM extraction.
- Optional contacts разрешён только для explicitly mapped coupled endpoints.
  Старые connect сохраняют прежний контракт; для S/Z/Y/delay single-contact
  режим отвергать. Никакого произвольного нового connection DSL.
- Для external port, закороченного weld, текущий адресный отказ допустим;
  не выдавать удалённое измерение за 0 V с полноценной validity.
- После реализации: focused independent coupons, прямой DSL (both/single
  contact и ненулевой charge), installed package, negative/identity/budget
  проверки и PNG. Никаких полных PCB solves; прежние numeric gates сохраняются.
- Не коммитить до parent review. Завершить пакетом и остановиться.

Production entry: `native/analyze/src/electrical/coupled/composition.rs`;
transient integration: существующие `coupled/time.rs` и `delayed.rs`;
frequency integration: `coupled/frequency.rs`. Нового time solver нет.
Ниже предложение, ещё не реализованное. Исходное задание скопировано из
`f320bf9`; accepted mixed `9f10f9c` не переносился повторно.

### 1. Минимальный контракт и явные контакты

В `coupled-rlc.data` добавить optional `portTerminals`, массив в **порядке
Resource.ports**. Каждый элемент содержит positive/negative, каждый контакт —
либо `{node: string}`, либо `{reference: 'ground'}`. Имена node ссылаются строго
на `potentialNodes` данного instance, не на Resource id и не на общий namespace.
`reference: 'ground'` допустим только для `referenceTopology: 'grounded'` и
означает автором явно объявленный физический return с заданным потенциалом 0.
Это не infinity, не новый potential node и не автоматически доступная земля.
Infinity допускает только два разных сохранённых node. Не выводить контакты
из численного B даже если это возможно: metadata — авторское утверждение.

Один полный предлагаемый TypeScript Resource и существующий способ соединения:

```ts
const cell = {
  id: 'physical-cell', kind: 'network', source: 'Authored full conductor model',
  analyses: ['networkParameters', 'si'], ports: ['join', 'probe'],
  data: {
    format: 'coupled-rlc', referenceTopology: 'infinity',
    potentialNodes: ['p', 'n'], currentDofs: ['branch'],
    incidence: [[0, 0, 1], [1, 0, -1]],
    portIncidence: [[0, 0, 1], [1, 0, -1], [0, 1, 1], [1, 1, -1]],
    resistanceOhm: [[0, 0, 100]], inductanceH: [[0, 0, 1e-6]],
    capacitanceF: [[0, 0, 3e-9], [0, 1, -2e-9],
                   [1, 0, -2e-9], [1, 1, 3e-9]],
    conservedPotentialModes: [[1, 1]],
    portTerminals: [
      {positive: {node: 'p'}, negative: {node: 'n'}},
      {positive: {node: 'p'}, negative: {node: 'n'}}
    ]
  }
} satisfies NetworkResource;
const left = a.network({model: cell, ports: {join: 'join', probe: 'probe'}});
const right = a.network({model: cell, ports: {join: 'join', probe: 'probe'}});
const joined = a.connect({
  networks: [left, right],
  connections: [{from: left.port('join'), to: right.port('join')}],
  ports: {left: left.port('probe'), right: right.port('probe')}
});
```

С mapped coupled endpoints эта пара означает два weld: positive↔positive и
negative↔negative. Старый grounded/canonical путь без metadata сохраняется
для старых графов; смешивать его implicit return с mapped infinity запрещено.
Два instances одного Resource остаются разными наборами проводников.

Для проверяемого **одного** контакта нужен ещё один небольшой явный параметр
существующего `a.connect`, а не новый builder: optional
`contacts: 'both' | 'positive' | 'negative'` у connection, default `'both'`.
Например `{from: left.port('join'), to: right.port('join'), contacts:'positive'}`
связывает только p. Port pair по-прежнему потребляется один раз; оставшийся
контакт не становится ground или отдельным доступным port. При single contact
не накладывать старые differential voltage/current pair constraints: KCL
получается из weld проводников. Поэтому этот режим допустим только для двух
полностью mapped coupled endpoints, не для произвольных S/Z/Y ports.

Native DTO, camelCase/denyUnknownFields; Model получает только новое поле:

```rust
#[derive(Clone, Deserialize, Serialize)]
#[serde(untagged)] // каждый inner struct также deny_unknown_fields
enum PhysicalTerminal { Node(NodeTerminal), Ground(GroundTerminal) }
// struct NodeTerminal { node: String }
// struct GroundTerminal { reference: GroundReference } // enum: "ground"
struct PortTerminals {
    positive: PhysicalTerminal,
    negative: PhysicalTerminal,
}
// Model: #[serde(default)] port_terminals: Option<Vec<PortTerminals>>
// Topology.connections: прежние Vec<[usize; 2]>.
// Topology: optional connectionContacts: Vec<ContactSelection>,
// где enum = "both" | "positive" | "negative"; если задан, длина == connections.
// TS flattening переносит contacts без изменения порядка pairs/instances.
```

Это единственная предлагаемая добавка к connection API; необходима, поскольку
исходная пара ports не выражает single-contact weld. Не расширять её до
arbitrary joins, terminal graph DSL или general connection framework.

Admission до clone/large allocation: ровно один map entry на каждый port;
уникальные potential node names; неизвестный/повторный контакт внутри пары,
неизвестный reference, дополнительные поля, неверные размеры — reject.
После coalescing sparse B каждый столбец должен **точно** совпасть с
`e_positive - e_negative` (ground отсутствует в векторе), включая знак и нули;
коэффициентные tolerance здесь не подменяют идентичность authored incidence.
Несовпадение — `COUPLED_TERMINAL_MAP`. Если weld замыкает external port в ноль,
reject с указанием контактов; не удалять такой port молча.

General/noncanonical B или reduced basis первоначально отвергать для physical
composition, сохранив их прежний single-block evaluator. Для будущего допуска
нужны полный trace `v_terminal = T x`, физические conductor ids/orientations,
оператор charge recovery и full exterior-C energy, retained/nullspace metadata
и доказательство точности reduction для всех объединяемых terminal traces.
Один B задаёт power pairing, но не восстанавливает эти данные. Даже известный T
не даёт права выбросить неизвестные заряды/моды; этот этап их не производит.

### 2. Assembly, состояния и charge/gauge

Знак существующего descriptor не менять:
`C dv/dt + A j = B i_p`, `L dj/dt - Aᵀv + Rj = 0`,
`u_p = Bᵀv`; `i_p > 0` входит в positive conductor и выходит из negative.
При физическом weld отдельного закона `i_left = -i_right` для исходных
потреблённых modal ports нет: их переменные исключаются, а ток через weld
следует из conductor KCL. Оставшиеся external ports имеют прежний power
`u_p i_p`. Branch orientation сохраняет authored A/R/L.

Union-find объединяет только выбранные physical contacts. Пусть P — sparse
0/1 recovery исходных potential nodes из assembled V, а grounded rows имеют
значение 0 и не становятся unknown. Тогда
`v = P V`, `C* = Pᵀ C P`, `A* = Pᵀ A`, `B* = Pᵀ B`;
R/L остаются block diagonal по независимым branch DOFs, включая взаимные
коэффициенты внутри блоков. `E* = diag(C*, L)`; никакого Schur удаления
exterior C. Weld с явно объявленным ground разрешает только **явное**
зануление соответствующего физического проводника; grounding никогда не
следует из слова infinity или из двух differential ports.

`q_local = C v`, `q_assembled = Pᵀ q_local`; все исходные exterior charges
остаются в этой сумме. Для любого настоящего conserved potential mode w:
`d(wᵀ C* V)/dt = wᵀ B* i_p`, когда `A*ᵀw = 0` и учтены нагрузки/источники.
При отсутствии внешней утечки заряд сохраняется; глобальное `sum(q)=0`
не добавлять. Кандидаты modes переносить через equality constraints P и
повторно проверять на assembled equations, как в текущем composition.
`G w=0, E w=0` — mathematical gauge: разрешена существующая border constraint.
`G w=0, E w≠0` — физический charge mode: фиксировать из initial state,
никогда не занулять в качестве gauge. Нулевой AC common-mode increment
не означает нулевой исходный DC charge.

Initial state задаётся **после** определения topology. Минимальное расширение
существующего native `Request.initial` и SI config.initial:

```ts
initial: {
  kind: 'physical-state', steady: false,
  potentialsV: [
    {instanceId: left.id, node: 'p', value: 2},
    {instanceId: left.id, node: 'n', value: 1},
    {instanceId: right.id, node: 'p', value: 2},
    {instanceId: right.id, node: 'n', value: 1}
  ],
  currentsA: [
    {instanceId: left.id, dof: 'branch', value: 0},
    {instanceId: right.id, dof: 'branch', value: 0}
  ]
}
```

`steady:false` допускает consistent coupled transient; приведённые значения
с R/L branches требуют именно этого режима (при `steady:true` эта запись
отвергается: branch voltage 1 V и j=0 не являются steady). Для constant delay
history разрешён только `steady:true` и полная проверка общего G0, источников
в t0 и wave constraints. До physical delay trace такой mixed-infinity граф
всё равно reject, см. ниже. Существующие `'steady'`/continuation не переименовывать.

Все retained nodes/branch DOFs должны присутствовать ровно один раз по
instance identity; finite F64, weld values согласованы. Инициализатор проверяет
algebraic consistency без изменения charge/flux. Запретить automatic averaging
изолированных snapshots с разными weld voltages: это импульсное соединение
заряженных конденсаторов с неописанной потерей энергии. Разногласие —
`COUPLED_INITIAL`; недоопределённый физический steady charge без state —
`COUPLED_INITIAL_CHARGE`. Полный state делает charge явным без нового API
для выбора произвольного modal basis. Для старых grounded графов семантика
initial сохраняется. Для DC, если исходные equations несовместны (например,
ненулевой постоянный ток в изолированный capacitor), reject `COUPLED_DC`;
border/gauge не должен превращать его в допустимый steady.

Энергия `U=.5 Vᵀ C* V + .5 jᵀ L j` и Joule `jᵀRj` идут в существующие
accounting/BE solver. Сохраняются Dbe как numerical dissipation, raw balance
без Dbe, refinement и отдельный signed-history diagnostic; критерии принятого
mixed backend не ослаблять. В отчёт добавить conductor equivalence/recovery,
explicit ground welds, initial/remaining conductor charges и proven gauges;
показывать actual initial charge/energy, а не только differential port traces.

### 3. Grounded/delay и provenance

Mapped grounded coupled block может соединиться с infinity только с явно
указанным physical return `{reference:'ground'}`; negative weld тогда явно
ground-ит выбранный infinity conductor. Сопоставление return нельзя вывести
из B, блока `'common'` или совпадения названий. Частично mapped граф reject.
Grounded/delay старые homogeneous graphs остаются на принятом пути.

У текущего delayed-scattering Resource есть differential waves и общий
reference, но нет полного conductor trace/common-charge storage. Поэтому
не принимать его physical weld с infinity даже при одинаковых port voltages:
`COUPLED_TERMINAL_MAP` с missing physical return/trace. Возврат такой связи
возможен лишь после авторского physical terminal/return и charge/storage
контракта; в данном этапе его не изобретать и не менять external P7 sources.
P7 выдаёт Resource; P6 собирает его в existing backend. Frequency wave-only
composition не является обходом запрета физической infinity assembly:
mapped infinity coupled граф идёт через assembled descriptor, не common_tables.

Сохранить исходные Resource bytes/hash links, source, analyses, validity/domain,
band intersection, conditional flag и instance-to-source mapping. Поля map,
contact selection и initial state входят в topology/state identity; continuation
после их изменения reject. SI permission проверять до native execution.
Переименование/перестановка node не меняет semantics, но создаёт новый hash;
не объявлять это identity-compatible continuation без явного recovery.

### 4. Независимые coupons и численные gates

Fixtures — небольшие authored matrices, не PCB solves. Reference не вызывает
production composer, его union-find, descriptor solver или charge helpers.

1. Два full-C two-node блока: `C1=[[3,-2],[-2,3]] nF`,
   `C2=[[7,-3],[-3,7]] nF`; в каждом branch p→n: R=100 Ω, L=1 µH,
   initial p=2 V,n=1 V,j=0. Both-contact weld даёт независимо вручную
   `C*=[[10,-5],[-5,10]] nF`, два branches; сравнить с **одним** authored
   исходным Resource этой схемы и отдельным analytic solution.
   Initial local q1=(4,-1) nC, q2=(11,1) nC; assembled q=(15,0) nC,
   U=15 nJ. Common voltage=1.5 V, conserved total Q=15 nC; steady U=11.25 nJ,
   differential energy/Joule limit=3.75 nJ. Independent analytic reference:
   Req=50 Ω, Leq=.5 µH, Cdiff=7.5 nF;
   `δ''+(Req/Leq)δ'+δ/(Leq*Cdiff)=0`, δ(0)=1, δ'(0)=0;
   `V=(1.5+δ/2,1.5-δ/2)`, branch currents from `-Cdiff*δ'/2` each.
   Common-charge variant all potentials=2 V, j=0 is constant:
   Q=20 nC,U=20 nJ, no hidden discharge/ground.

2. Те же исходные blocks, weld **только p**. Независимая трёхузловая схема:
   `C=[[10,-2,-3],[-2,3,0],[-3,0,7]] nF`, nodes=(p,n1,n2),
   branches=(p→n1,p→n2). Reference — отдельный small dense matrix exponential
   по напрямую выписанной ODE, с проверкой сходимости независимого Taylor
   scaling/squaring; не wrapper над production MNA. При t=0 n1=n2=1,
   `(n2-n1)''=-5/31*1e15 V/s²`, тогда как both-contact n1=n2 тождественно.
   Сравнить trajectory до 5 µs, Q=15 nC и initial U=15 nJ; all=2 V опять
   остаётся constant Q=20 nC,U=20 nJ. Negative-only weld — перестановочный
   аналог. Проверить, что сборка не добавила ground row/constraint.

3. Переименовать и переставить potentialNodes, C/A/B/maps/state согласованно;
   результат привести обратно по recovery и сравнить. Обратить ориентацию
   обоих соединённых ports: swap positive/negative, negate B columns,
   соответствующие measured voltage/current signs; физическая схема и U/Q
   неизменны. Отдельно single-block external orientation с signed driver.
   Обратить только один endpoint — другая wiring, не ложная invariance.

4. Negatives: B/map sign/extra-entry mismatch; missing map/return; map to
   unknown node; infinity→implicit grounded/delay return; general B и reduced
   trace без recovery; несовместимые initial weld voltages; неполный physical
   initial charge при floating steady; inconsistent steady state; isolated
   1 nF capacitor с DC current=1 mA. Последний reference: `dv/dt=1e6 V/s`,
   конечного DC steady нет. Решатель обязан reject, не поглотить ток gauge.
   Отдельный C=0 common null mode должен быть признан только zero-energy gauge.

Numerical acceptance для fixtures: assembled coefficient comparison
`|Δ| <= 1e-14*max|M| + 1e-24` в единицах matrix; F64 scaled physical residual,
KCL и weld mismatch <=1e-9 относительно своей ненулевой нормы
(absolute floors 1e-12 V/1e-15 A). Charge error <=`1e-8*max|Q0|+1e-18 C`,
energy initial/reference <=`1e-8*max U+1e-20 J`. Transient against analytic
reference <=0.25 mV, удвоение точности timestep даёт <=0.125 mV либо минимум
двукратное уменьшение ошибки (если ещё не ниже 1 µV); output grid не timestep.
Общая energy accounting bound прежняя: normalized balance after Dbe <=1e-4,
zero norm absolute<=1e-20 J; публиковать также raw balance и refinement raw
defect, charge drift. Frequency reference 1 kHz..100 MHz: complex differential
response relative<=1e-8 (absolute floor1e-10 в единицах response), сохранение
common-charge offset отдельно; frequency perturbation не заменяет transient Q.

### 5. Реализация после review, ресурсы и остаток

1. TS `frequency-model.ts`/spec-doc.d.ts + native Model/Topology DTO, strict
   validation и bounded flattening contacts; `transient-executor.ts` переносит
   physical initial и provenance, native declarations синхронизированы.
2. В `coupled/composition.rs` одна conductor recovery/assembly для coupled
   frequency/time. В `coupled/mod.rs` использовать прежние physical constraints,
   original-equation residual, charge/energy accounting; расширить initial в
   `coupled/time.rs`. В `delayed.rs` переиспользовать admission/recovery где
   применимо, сохранив явный reject неизвестного delay trace.
3. Native focused coupons + независимый reference script/artifact; TS public
   a.connect both/single contacts, strict negatives, source permissions/domain;
   затем isolated addon/public/installed smoke. Existing mixed regression один
   раз только после этих изменений; старые accepted evidence не переписывать.
4. Сохранить numeric report, source/recovery/state hashes, две PNG trajectories
   (both vs one; common charge/energy), inspection и exact commands/results;
   parent review до любого commit. Не менять placement F32/GPU/sharedaddon.

Документация: builds/solves **не запускались**. Будущий heavy этап — только
после review и согласованного слота, cargo jobs1/isolated target. Coupon n<=5
descriptor states, <=4 ports, <=10001 delivered samples; maxStep<=1 ns
и refinement<=.5 ns, stop5 µs, с дальнейшей адаптацией по errors.
Один shared deadline/maxSeconds=60, maxMemoryMiB=256 на native coupon request;
полный focused пакет целевой <=120 s без build, addon build отдельно.
Admission до sparse/dense materialization: прежние caps 32blocks/128internal
ports/16external, nv1024/ni1536/states2048/4M entries; map<=2*128 contacts,
union metadata O(nv+ports), sparse recovery <=nv entries, conductor report
<=nv+2*ports records. Матрицы/factors/constraints по прежнему консервативному
`(states+nv)^2*1280` + resource copies + outputs/history; уменьшение размерности
после weld не должно обходить preassembly budget. Для coupons worst planned
matrix reserve<128 KiB, output traces<32 MiB при 10001 samples; всё вместе
<=64 MiB native planned, OS RSS addon/Node измерять отдельно. До Rust allocation
отвергать budget exhaustion; не объявлять успешный planned-memory actual RSS.

| Остаток | Граница после предлагаемого этапа |
|---|---|
| P6 physical terminals | Full unreduced coupled, explicit both/single contact weld, nonzero charge, grounded return только явно; требует реализации и review |
| P6 mixed delay/RLC | Принят; infinity+delay остаётся blocked до physical return/trace, второго solver нет |
| P6 general B/reduced | Single-block прежний; physical assembly blocked без полного trace/charge contract |
| P6 arbitrary joins/resonant fit | Вне задания; не открывать |
| P7 | Producer contract/geometry accuracy отдельно, P6 time backend не дублировать |
| P8/P11 | Не закрыты и этим checkpoint не проверены |

Историческая остановка design checkpoint снята parent approval8a8369e выше.

## Implementation handoff — 2026-10-24

Bounded physical-terminal package готов к parent review, без commit.
Результаты: [electrical-physical-verification.md](../../evidence/electrical-physical-verification.md);
ignored evidence: `debugging/p6-physical-verification/manifest.json` и
`metrics.json`, исходные JSON, native logs, installed package, две inspected PNG.

- Existing conductor composer применяет full C/A/B congruence, explicit ground
  recovery и both/single contact weld; unknown returns/traces/delay joins и
  shorted external ports адресно отвергаются. Отсутствующие intermodel mutual
  C/L не извлекаются; это явно указано в semantics, source и reports.
- Physical initial state/charge, exact zero-energy gauges, continuation seed,
  source permissions/domains и hash links сохранены. SI initial допускает
  passive unforced decay; maxStepS — только coupled descriptor. Старый grounded
  path и null-seed continuation проверены. Output axis переиспользует existing
  electrical output_samples, без duplicate final sample.
- Native physical8/8, coupled9/9, mixed13/13, delay4/4, admission2/2; TS4/4
  безskip, independent saved audit и11corruptions; typecheck/build PASS.
- Direct DSL и installed compiled worker:8 calculations+6refusals каждый,
  cancellation/recovery и normal shared-addon f32-rte-ftz-v1 PASS. Max error
  106.490µV→53.194µV; charge drift8.934e-23C, normalized energy2.118e-14.
- Две PNG просмотрены; exact full model/source/artifact hashes в manifest.
  Late continuation с tiny energy exchange на большом charged rail может
  честно reject COUPLED_ENERGY: floor/charge/gauge не подгонялись радиacceptance.

Heavy освобождён P7. Предыдущие ignored evidence сохранены; placement F32/GPU,
main и другие worktrees не изменялись. P6 arbitrary joins/general B/reduced
trace/infinity+delay и P8/P11 не закрыты. Остановиться до parent review.

### R1 follow-up — 2026-10-24

Единственный blocking review805f183 исправлен: legacy energy guard сохранён
conjunction с normalized guard, dissipation nonnegative, small/zero floor не
ослаблен. Predicate1/1 +physical8/8 +coupled9/9; latest addon/direct/installed
и TS4/4 PASS. До/после численные arrays/quality точно одинаковы; old evidence
сохранены. Latest hashes — root manifest, новые результаты — `r1/`.
См. [R1 response](../../evidence/review-physical-energy-guard.md) и verification R1 section.
Scope только R1; без PCB reruns/commit. Heavy освобождён Thermal, остановиться.
