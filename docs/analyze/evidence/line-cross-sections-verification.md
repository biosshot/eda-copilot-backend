# P7a: проверка численных сечений — 2026-10-23

Реализация [задания](../archive/tasks/next-task-line-cross-sections.md) передана на ревью,
**без коммита**. База `2c7d31882cc12ab9290427f1f5592cea8b53b58b`, ветка
`feat/analyze`. Все четыре LineInput рассчитываются собственным CPU/F64 P1 FEM;
[семантика, таблицы, caps и границы](../reference/line-cross-sections-semantics.md).

## Исправление R1: полнота графиков — 2026-10-23

По [ревью R1](line-cross-sections-review-2026-10-23.md) исправлен только аудит
и его тесты; solver/native/DSL/геометрия, входные fixtures и численные допуски
не изменены. Общий `expectedSeries` требует ровно один график каждого ожидаемого
названия, `kind: line`, `data.kind: series`, правильные units/conditional validity,
полную ось `frequencyHz`/`Hz` и массив такой же длины. Индексный проход проверяет
конечность каждого отсчёта, включая разреженные массивы, затем прежние формулы
сравнивают значения с network result с прежними допусками.

Команды ниже завершились **exit 0** на Windows x64:

```powershell
node --import tsx --test --test-concurrency=1 tests/pcb-analysis-cross-section-audit.test.ts
npm run typecheck
node --import tsx debugging/analysis-dsl/section-r1-reaudit.ts
git diff --check
```

- **3 focused audit tests** прошли: прежние 16 report + 7 source подмен,
  дополнительно **144 отрицательных случая** (18 вариантов × magnitude/phase ×
  S21, Near/Far crosstalk и Line + Attenuator Pad S21). Покрыты пустой,
  укороченный и удлинённый массив при сохранённой оси, дубликат графика,
  sparse/undefined/null/string/NaN/Infinity, отсутствие массива, неправильные
  kinds, unit, validity и имя/единица/значения оси. Полные production reports
  BandAmp и STM32 проходят до и после подмен.
- Отдельный **повторный аудит сохранённых JSON**, без нового simulation run:
  BandAmpV5 — pass, 6 series; STM32G0_CAN_RS485 — pass, 8; ThunderF722 — pass, 4.
  Для каждого из всех 18 графиков отвергнуты empty/truncated/extra/duplicate:
  **72 повреждённые копии**. Проверены SHA256 всех файлов исходных run directories
  до/после, сохранённых плат и native binary; исходные отчёты, hashes и run metadata
  не перезаписывались.
- Логи: `debugging/analysis-dsl/section-r1-{audit-tests,typecheck,reaudit}.log`.
  [Отдельное evidence повторного аудита](../../../debugging/analysis-dsl/section-r1-reaudit.json)
  содержит текущие audit/test hashes, исходные simulation metadata, hashes файлов
  и результат каждой отрицательной проверки. Прежние evidence/manifest ниже
  относятся к исходной реализации P7a и не перетегированы как новая версия аудита.

Новые focused tests выполняют два коротких production-сценария через существующий
addon; это отдельно от повторного аудита исторических JSON. FEM/native не менялись
и не пересобирались. Прежние полные Rust/Node/DC/RLC/P6b проверки и Linux/macOS
повторно не запускались. Коммит и следующий этап отложены до повторного ревью.

## Проверки

Все нижеприведённые команды завершились **exit 0** на Windows x64.

```powershell
$env:PCB_BLOCK_BACKEND='cpu'
$env:PCB_BOARD_BACKEND='cpu'
$env:PCB_POST_PLACE_BACKEND='cpu'
cargo test --release --locked --manifest-path native/analyze/Cargo.toml --test cross_section --test frequency --test electrical --test dc --test conduction -- --test-threads=1
npm run native:build
npm run typecheck
npm run build
node --import tsx --test --test-concurrency=1 tests/analysis-dsl.test.ts tests/pcb-analysis-dc.test.ts tests/pcb-analysis-dc-ports.test.ts tests/pcb-analysis-dc-audit.test.ts tests/pcb-analysis-electrical.test.ts tests/pcb-analysis-rlc-audit.test.ts tests/pcb-analysis-frequency.test.ts tests/pcb-analysis-frequency-audit.test.ts tests/pcb-analysis-cross-section.test.ts tests/pcb-analysis-cross-section-audit.test.ts tests/pcb-analysis-conduction.test.ts tests/pcb-analysis-geometry.test.ts tests/pcb-analysis-runtime.test.ts tests/pcb-native-addon.test.ts tests/public-api.test.ts
node --test scripts/test-pcb-analyze.test.mjs
npm run test:package
npm run test:pcb-analyze -- BandAmpV5 STM32G0_CAN_RS485 ThunderF722 --name impedance
git diff --check
git apply --reverse --check debugging/analysis-dsl/section-review/changes.patch
```

- **73 Rust**: 9 cross-section, 20 conduction, 12 DC, 17 electrical/P6a, 15 P6b.
- **175 Node**: 7 новых production cross-section и 2 audit tests; сохранены
  прежние DC/RLC/P6b/runtime/strict addon/public API проверки.
- **4 launcher**; native/typecheck/build/installed package прошли.
- Installed package проверен обычным запуском, с GPU disabled, с CUDA/Vulkan
  unavailable. В каждом режиме выполняется настоящий FEM → `z.network` → P6b,
  плюс прежние placement/F32/worker/import checks. Публичные типы и новый mesh
  builder компилируются в отдельном installed consumer без casts.
- Root addon exports/версии прежние; добавлен только метод существующего
  AnalysisElectricalSession. Cargo dependencies/features/lockfiles не изменены.
- Node/Rust standalone tool сверены по parameters, quality, counts и diagnostics;
  изменение frequency axis не меняет field/FEM. Новая длина меняет propagation,
  новая ширина меняет C; старый network остаётся неизменным.
- Negative cases: потери/материалы/mask/неподдержанные targets/методы,
  geometric/frequency/memory/mesh/time caps, report limit, отмена/deadline,
  последующий успешный запуск. Крайние положительные ε, приводящие к overflow/underflow
  модального Z/delay, дают SECTION_NONFINITE вместо JSON null/panic; отдельный
  отрицательный Rust regression. Guard также проверяет discrete maximum principle,
  конечность energy integrals и всех E перед сериализацией. Missing length даёт unknown delay и недоступный
  network; fail/unknown не останавливают следующие checks.

Логи: `debugging/analysis-dsl/section-{rust,node,native,typecheck,build,package,fixtures,launcher}-final.log`.
`section-diff-check.log` содержит только LF→CRLF предупреждение Git для lib.rs,
не whitespace failure. В native build прежние 20 packer warnings и linker message;
новых предупреждений extractor нет. Полный `npm test`, Linux/macOS/CI здесь не запускались.

## Численные сравнения

- Аналитические однослойные и многослойные parallel plates с natural side BC:
  тот же field kernel, точный C (однослойный проверяется до 1e-10 relative,
  многослойный — 1%), энергия/charge checks; mm→m.
- Air LC и matrix C0L invariants, reflection symmetry/перестановка conductors
  и RHS/полей, знаки mutual C/L, weak/strong gap и предел isolated stripline.
- Heterogeneous stripline проверен thin-interface/symmetric-height average-epsilon
  пределом, затем изменением высоты; это отдельно от опубликованного homogeneous
  stripline reference. Не выдаётся за независимый произвольный multilayer solver.
- **10 независимых engineering cases**: Qucs single/coupled static equations и
  опубликованная Ansys stripline table. Геометрия/единицы/формулы/полоса применимости
  versioned в `tests/fixtures/analysis-cross-section/references.json`.
  Thin-conductor и finite-thickness случаи разделены. Max relative Z error
  **0.442510%** при согласованном допуске **3%**. Полные actual/expected C/L/modes,
  количества элементов и guards: `debugging/analysis-dsl/section-reference-comparison.json`;
  этот артефакт получен прямым вызовом production Rust example на сохранённых
  reference inputs, без второго solver.
- Независимая reflected-wave series проверяет analytic propagation (полный
  4-port oracle), matched, reflections, quarter/half wave, длину и phases modulo
  комплексного ответа: **1e-8 relative + 1e-10 absolute**. Не смешивается с FEM error.
- В трёх финальных board reports max complex oracle error
  **5.566705740848049e-16**, max actual FEM residual **2.118128349127986e-15**,
  energy/reaction и charge guards лучше 1e-8. Каждая принятая сетка и outer-domain
  проверка отдельно удовлетворяет 1%; допуски не повышались.
- Все матрицы P6b получены на точной заданной axis. При inter-point query остаётся
  прежняя conditional interpolation, не новое точное решение длинной линии.

## Прямые примеры и измеренные затраты

| Плата | Native FEM+propagation, s | Scenario wall, s | RSS*, MiB | Nodes / triangles | Последние mesh / domain changes | Z0/modes, Ohm |
| --- | ---: | ---: | ---: | --- | --- | --- |
| BandAmpV5 | 0.0627 | 0.7890 | 126.20 | 7050 / 13696 | 0.9138% / 0.4201% | single 53.180806 |
| STM32G0_CAN_RS485 | 0.4004 | 1.9557 | 317.34 | 46495 / 91872 | 0.9847% / 0.6973% | even 94.841559, odd 66.568360 |
| ThunderF722 | 0.2058 | 1.2418 | 206.41 | 22947 / 45160 | 0.4837% / 0.7047% | single 60.443694 |

*RSS — измеренный OS process high-water **до последнего native вызова** внутри
сценария (включая Node/предыдущие вычисления). Финальная IPC serialization после
этой точки отдельно не измерена. Это не строгий OS memory cap. Планы sparse factor
values/workspaces и mesh admission сохранены в field quality/limits.

- BandAmpV5: TOP RF_IN_PORT `dcc17162579803b7`, 0.6736×10.6299 mm,
  hypothetical h=.4 mm, ε=4.2; отдельный каскад с заданным Attenuator Pad.
- STM32G0_CAN_RS485: реальные TOP SWCLK/SWDIO `5d83ad1362a81e70` /
  `f91dce6599297e96`, ширина .2, gap .3002, общий прямой отрезок 3.051 mm;
  hypothetical h=.3 mm, ε=4.2. Это соседние линии, не объявленный differential bus.
  Even/odd, Zdiff=133.136720 Ohm, Zcommon=47.420780 Ohm; задержки различаются.
- ThunderF722: BOTTOM UART5_RX `e216`, .254×8.0009 mm,
  hypothetical h=.2 mm, ε=4.2.

Во всех трёх t=.035 mm, PEC/no mask/lossTangent=0 — **явные hypotheses**,
идеальная reference не заявлена проверенной по реальной меди. V2 JSON не менялся.
Отчёты conditional, по **1 expected fail + 1 выполненный pass**, required unknown
нет. **18 series plot arrays** проверены audit, включая каскад BandAmp.
Намеренные 23 audit-подмены (16 report + 7 source geometry) отвергаются.

Итоговые JSON sizes: BandAmp 1.234 MiB, STM32 12.544 MiB, Thunder 3.954 MiB.
Стандартные 4 MiB runner report budget сохранены; подробные named fixtures задают
ограниченные 128 MiB. Компактный прямой пример из задания проходит с default budget.

При разработке повторные прогоны проводились после изменений solver/guards и
исправлений renderer: убраны raster cracks без сетки и разнесены перекрывавшиеся
подписи пары. В окончательных runs использована одна и та же финальная native
библиотека; прежние runs не перезаписаны и не выданы за актуальные.

## Артефакты и визуальная проверка

- [Индекс трёх отчётов](../../../debugging/pcb-analyze/impedance/index.html).
- [Summary / hashes](../../../debugging/analysis-dsl/section-results.json),
  [source parity](../../../debugging/analysis-dsl/section-source-parity.json),
  [review diff](../../../debugging/analysis-dsl/section-review/changes.patch).
- [BandAmpV5](../../../debugging/pcb-analyze/BandAmpV5/runs/2026-10-23T10-34-02-779Z-impedance-I1CFjx/index.html).
- [STM32G0_CAN_RS485](../../../debugging/pcb-analyze/STM32G0_CAN_RS485/runs/2026-10-23T10-34-03-975Z-impedance-GY4ItM/index.html).
- [ThunderF722](../../../debugging/pcb-analyze/ThunderF722/runs/2026-10-23T10-34-06-543Z-impedance-qyIziK/index.html).

В каждом timestamp-name-unique run: exact scenario/resources/expectations,
implementation hashes, board/script/native hashes, JSON, audit, HTML, run log.
**21 screenshots** в `debugging/pcb-analyze/impedance/screenshots/`: board IDs,
phi/mesh, оба независимых возбуждения пары, E, mesh/domain convergence, C/L/modes,
S21 phase, crosstalk magnitude/phase и fail→continue. Все три страницы открыты и
визуально просмотрены; select/mesh toggle проверены. HTML self-contained,
численные массивы сохранены; временный localhost server и tab закрыты.
После визуального просмотра добавлен finite-range guard, пересобран addon и
повторены 73 Rust / 175 Node / package / named runs. Точные SHA256 всех normal
field/mesh/C/L/convergence/network/plot arrays совпали с визуально просмотренными
версиями: [visual parity](../../../debugging/analysis-dsl/section-visual-parity.json).
Это отдельное доказательство соответствия screenshots, а не подмена их provenance.

Native SHA256:
`256b2163dbe43f8cf1ecb6e778887cf8f0b13cfbade8eb255b76d673fa56eb6a`.
Archive SHA256:
`85b88a8b55b509c0bb6f0838a311ec211c29663bfcbeb414b322c054ac97c782`.
Installed consumer:
`C:\Users\kiril\AppData\Local\Temp\eda-backend-package-wGsOmt`.
Installed binary SHA совпадает с текущим production `.node`.

## Сохранённые границы

117 прежних Rust/fixture files byte-identical HEAD с нормализацией EOL: placement,
DC/conduction/copper/import, MNA и P6b, сохранённые board JSON. Аддитивный
transport/dispatch и новая extraction вынесены за этот parity set. Старые тяжёлые
8 DC / 3 RLC board corpora заново не считались, их прежнее evidence не перетегировано;
пройдены focused regressions и installed-package сценарии.

Нет real conductor/dielectric losses, internal L/roughness, dispersion/mask,
via/plane gaps, finite ground/radiation, automatic PCB path/PDN extraction,
full-wave, FFT/fitting/TDR/time/SI/eye/BER, F32/GPU/cache или общего runtime.
Geometric quasi-TEM screening — эвристика, FEM convergence не доказывает full-wave
accuracy. Оставшийся P6/P7/P11 не закрыт; следующий этап не начинался, commit/staging нет.
