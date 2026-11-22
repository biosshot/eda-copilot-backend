# Задание: исправить DC и привести тесты плат к прямому Analysis DSL

Дата: **2026-10-22**. Основание: [ревью D1–D3](../../evidence/dc-analysis-review-2026-10-22.md) и уточнение пользователя о формате примеров. Это исправление текущего этапа, **не следующее задание на новый анализатор**. M1 проверки единиц уже закрыто в `5b07c33`.

## 1. Цель, границы и порядок

Нужны четыре результата:

1. D1: объявления посторонних портов и пассивные измерения не меняют электрическую постановку DC.
2. D2: каждый результат содержит или корректно адресует геометрию всех своих рассчитанных областей.
3. D3: проверки численных результатов реальных плат хранятся в репозитории и запускаются вместе с корпусом.
4. Файлы примеров плат содержат непосредственно Analysis DSL, как PCB Layout, без `SavedDcScenario` и генератора текста сценария.

Сначала прочитать актуальные `AGENTS.md`, CHANGELOG, ревью, [отчёт реализации](../../evidence/dc-analysis-verification.md), код bridge/result/renderer и существующие тесты. Зафиксировать доступные исходные hashes и финальные отчёты корпуса как baseline; не запускать старый корпус повторно только для получения уже сохранённых результатов.

Рекомендуемый порядок: перенести audit в сохраняемые тесты → перевести fixtures на прямой DSL → добавить регрессии D1/D2 и исправить их → focused проверки → один финальный прогон изменившихся реальных сценариев → визуальный контроль.

**Не делать:** новый DSL/парсер, новый публичный runner, новый универсальный test framework, глобальный cache, другой sparse solver, новые анализаторы, расширение агрегированных источников DC, автоматический подбор моделей, CAD-refill. Не менять входные PCB, экспортёр, stackup/токи/пороги ради прохождения тестов. Не ослаблять numerical/readiness guards и не возвращать дорогие двойные геометрии. Не менять PCB placement/F32.

Допустимы необходимые изменения TS/Rust для разделения активных контактов и наблюдений, исправление ссылок результата и локальная адаптация renderer. Физические изменения должны быть объяснены; не скрывать их за переносом тестовых файлов.

## 2. Прямые DSL-файлы вместо второго контракта сценария

Целевая организация каждой платы:

```text
tests/pcb-analyze/STM32G0_CAN_RS485/
  STM32G0_CAN_RS485.json  # существующий CAD snapshot, не менять
  dc.js                  # authored Analysis DSL, основной читаемый пример
  dc.ts                  # тонкий launcher для существующего --name dc
  dc.expectations.json   # необязательно: только ожидания теста/лимиты запуска
  README.md
```

- `dc.js` — обычный JavaScript с `analysis`, `board`, `stackup`, `ic`, `pad`, `net`, `await`, функциями и builders. Не TS-файл, который export-ит JSON для генератора; не строковый шаблон в другом `.ts`.
- Нельзя заменять генератор `savedDcCode` другим генератором/обёрткой над собственным мини-языком. Нельзя вводить test-only globals для описания источников/нагрузок вместо публичного DSL.
- В `dc.js` видны источники, нагрузки, компоненты, stackup/materials, допущения, mesh, sweep, измерения, check и plot. Обычные локальные JS-функции/массивы допустимы.
- Launcher читает файл как текст и передаёт его **без преобразования** в `runAnalysisDsl({ board, code }, options)`. Сам файл не импортируется Node.js как модуль: globals и top-level await предоставляет существующий runner.
- В output сохраняется точный исполненный текст (`scenario.js` допустим как имя артефакта), его hash и исходный путь. В `filename` передаётся путь/имя authored DSL-файла для понятных ошибок по строкам.
- Общий helper оставляет загрузку fixtures, runner, provenance, audit и HTML. Он не решает, какие источники/пины/нагрузки добавить, и не внедряет скрытые stackup/пороги в код.
- Expectations хранят количество/идентичность ожидаемых случаев, допуски и ожидаемые check statuses; runtime options — timeout/output budget. Это не повторная копия физических входов и не замена DSL.
- После переноса удалить `SavedDcScenario`, `savedDcCode`, старые `dc-scenario.ts` и обновить их импорты, включая cancellation-тест. Сохранять только реально нужные общие helpers.

Тонкий launcher может выглядеть так (имя и сигнатура helper — предложение для реализации, сейчас не гарантированно существуют):

```ts
import { runSavedAnalysisDc } from '../../helpers/pcb-analysis-dc.ts';

await runSavedAnalysisDc({
  script: new URL('./dc.js', import.meta.url),
  expectations: new URL('./dc.expectations.json', import.meta.url),
  runner: { timeoutMs: 540_000, maxOutputBytes: 64 * 1024 * 1024 },
});
```

Существующая команда `npm run test:pcb-analyze -- STM32G0_CAN_RS485 --name dc` остаётся рабочей. Если рядом появляется metadata `.ts`, не показывать его в `--list` как отдельный runnable analysis. JSON для expectations позволяет избежать этой неоднозначности без переделки discovery.

## 3. Пример полного authored DSL для STM32-платы

Ниже образец для `STM32G0_CAN_RS485/dc.js`, основанный на текущих контактах и гипотезах `dc-scenario.ts`. Он задаёт два источника, четыре нагрузки, три измерения, две сетки и две нагрузки на каждой сетке. Это целевой пример для проверки после исправления D1/D2; при составлении задания он не исполнялся. Значения — тестовые предположения, не характеристики изделия.

```js
const a = analysis({ board });

a.log({
  level: "warning",
  message: "DC test assumptions",
  data: [
    "Идеальные источники 3.3/5 V на контактах U2; регулятор не моделируется",
    "Токи IC — гипотезы теста; повышенная нагрузка умножает их на два",
    "1.6 mm stack, 35 um copper, 25 um plating, 58 MS/m at 20 C — гипотезы",
    "Неизвестная свежесть CAD-заливки: результат условен для данного snapshot",
  ],
});

const stack = stackup({
  boardThicknessMm: 1.6,
  viaPlatingThicknessUm: 25,
  platingMaterial: "cu",
  diameterConvention: "finished_hole",
  layers: [
    { kind: "copper", name: "TOP", thicknessMm: 0.035, material: "cu" },
    { kind: "dielectric", name: "core", thicknessMm: 1.53 },
    { kind: "copper", name: "BOTTOM", thicknessMm: 0.035, material: "cu" },
  ],
});

const source3v3 = a.port({ positive: pad("U2", "5"), negative: pad("U2", "2") });
const source5v = a.port({ positive: pad("U2", "1"), negative: pad("U2", "2") });

const consumers = [
  { label: "MCU", port: a.port({ positive: pad("U3", "4"), negative: pad("U3", "7") }), referenceV: 3.3 },
  { label: "CAN logic", port: a.port({ positive: pad("U4", "5"), negative: pad("U4", "8") }), referenceV: 3.3 },
  { label: "RS485", port: a.port({ positive: pad("U5", "8"), negative: pad("U5", "5") }), referenceV: 3.3 },
];

const base = a.dc()
  .components([ic({ designator: "U3" }), ic({ designator: "U4" }), ic({ designator: "U5" })])
  .stackup(stack)
  .materials({ cu: {
    conductivitySM: 58e6,
    state: "constant isotropic at 20 C; test assumption",
    source: "explicit test profile, not BOM",
  } })
  .temperatureC(20)
  .inputPolicy({ cadFill: "snapshot" })
  .sources([
    { port: source3v3, voltageV: 3.3, model: "ideal" },
    { port: source5v, voltageV: 5, model: "ideal" },
  ])
  .budget({ maxSeconds: 500, maxMemoryMiB: 1024 });

// Ссылка на настоящие данные, уже сохранённые в solution результата.
// Не дублируем большие values в каждой plot-записи.
function plotField(title, field) {
  const { values, ...reference } = field;
  a.plot({ title, data: reference });
}

for (const maxAreaMm2 of [1, 0.5]) {
  await a.sweep({
    parameter: { name: "load-scale", unit: "1", values: [1, 2] },
    evaluate: async (scale) => {
      const caseId = `mesh-${maxAreaMm2}/load-${scale}`;
      const dc = await base
        .name(caseId)
        .mesh({ maxAreaMm2, minAngleDeg: 0, maxRefinementSteps: 64 })
        .loads([
          { designator: "U3", rail: net("+3V3"), return: net("GND"), currentA: 0.15 * scale },
          { designator: "U4", rail: net("+3V3"), return: net("GND"), pins: ["5"], currentA: 0.03 * scale },
          { designator: "U5", rail: net("+3V3"), return: net("GND"), currentA: 0.04 * scale },
          { designator: "U4", rail: net("+5V"), return: net("GND"), pins: ["3"], currentA: 0.04 * scale },
        ])
        .run();

      const measurements = consumers.map(({ label, port, referenceV }) => {
        const voltage = dc.voltage(port);
        // Сохраняем validity/diagnostics. Отсутствие числа не становится нулём.
        const drop = typeof voltage.value === "number"
          ? { ...voltage, value: Math.abs(referenceV - voltage.value) }
          : voltage;
        a.check({ name: `${caseId}/${label}/drop`, actual: drop, max: 0.003, unit: "V" });
        return { label, portId: port.id, referenceV, voltage, drop };
      });

      a.check({ name: `${caseId}/copper-loss`, actual: dc.losses, min: 0, unit: "W" });
      plotField(`${caseId}/voltage`, dc.voltageMap);
      plotField(`${caseId}/current-density`, dc.currentDensityMap);
      plotField(`${caseId}/loss`, dc.lossMap);
      a.log({
        level: "info",
        message: "dc.case",
        data: { caseId, resultId: dc.id, maxAreaMm2, loadScale: scale, measurements },
      });
      return dc.losses;
    },
  });
```

`dc.case` — соглашение тестового отчёта через существующий `a.log`, не новый метод DSL. Helper/renderer могут читать эти записи и completed results. Они не должны зависеть от прежнего `metadata.scenario.measure` или от извлечения команд из исходного текста. Описания `solutionId/field` в plots обязаны разрешаться в сохранённые физические данные; если этот путь меняется, изменить его согласованно, не рисовать пустую карту.

Для всех восьми fixtures аналогично перенести **существующую** постановку: USB_PD_Trigger_5A, TPS63060DSCR, STM32G0_CAN_RS485, ESPower, BandAmpV5, NanoScope, NanoSpec, ThunderF722. Сохранить F1=0.02 Ohm в USB, текущие контакты, нагрузки/возвраты/ручные значения, два mesh levels, ×1/×2 и индивидуальные budgets. Не копировать двухслойный стек STM32 на шестислойный Thunder. Все численные гипотезы явно видны в DSL/профилях и отчёте.

## 4. D1: разделить активные контакты и наблюдения

`a.port(...)` описывает контактную пару. Само объявление не является подключением физического электрода, источника или нагрузки.

- Активные terminals для native assembly выводить из sources/loads, разрешённых распределений IC и явно подключённых элементов текущего builder.
- Не обходить все context ports как обязательные активные контакты. Неиспользуемые aggregated ports не должны вызывать `UNSUPPORTED_DC_MODEL`. Если такой порт реально передан неподдержанному источнику/нагрузке — сохранить явный отказ.
- Измерения на активном электроде используют его потенциал. Для пассивного pad измерять уже рассчитанное поле без `join_all`, добавления проводимости/источника или изменения выбранных физических областей. Предпочтительно определить area-weighted среднее потенциала по доступной конечной области pad; точное правило, обработку отверстий и неполного покрытия записать в d.ts/документации. Не подменять среднее случайным первым mesh node.
- Порт вне рассчитанной области, неизвестное покрытие или несовместимые математические опоры дают unavailable с причиной. Не дорисовывать нулевую карту и не запускать скрытый дополнительный solve. Неподдержанное агрегированное **измерение** также может остаться unavailable.
- Standalone conductor не зависит от любых board-port descriptors в том же контексте.
- Сохранить API `dc.voltage(port)` и независимые builders. Не добавлять обязательный `.measurements()` только для обхода дефекта.

Регрессионный пример на существующем `couponSetup`:

```js
const before = await base.loads([{ port: load, currentA: 1 }]).run();

// Описание может использоваться другим анализом; в текущий DC не передано.
const unused = a.port({
  positive: [pad("J1", "1"), pad("U1", "1")],
  negative: pad("J1", "2"),
});

const after = await base.loads([{ port: load, currentA: 1 }]).run();
a.log({ level: "info", message: "port-invariance", data: {
  beforeV: before.voltage(load), afterV: after.voltage(load),
  beforeLoss: before.losses, afterLoss: after.losses,
} });
```

Host-test проверяет не только `complete`, но совпадение активных electrodes/числа неизвестных/участвующих областей и напряжений/потерь в заданном физическом допуске. Не сравнивать random result IDs/timings.

Дополнительно необходимы: обычный неиспользуемый порт на нерелевантном непригодном острове; standalone conductor с посторонним портом; пассивная широкая площадка на полосе с ненулевым градиентом, чтобы тест действительно обнаруживал её ошибочное превращение в эквипотенциальную перемычку. В последнем тесте board/mesh одинаковы до/после объявления probe; известная аналитическая/контрольная величина не меняется, наблюдение читается из неизменённого поля.

## 5. D2: корректные ссылки на сетку при смене участвующих областей

Разделять reuse native preparation, reuse factorization и reuse геометрии отчёта. Совпадения board/profile/mesh options недостаточно, если первый отчёт содержал только часть domains.

Самое небольшое допустимое решение: сохранить текущую compaction, но давать `meshReference` только если referenced result содержит все требуемые nodes/triangles для текущих domains. Если покрытия не хватает — сохранить соответствующую сетку в новом результате и обновить ссылку для следующих повторов. Не требуется придумывать общий формат patches графа артефактов. Другой компактный вариант допустим при тех же гарантиях.

Тестовая плата: две независимые цепи питания A и B с отдельными supply/return-полосами, непересекающейся медью и однозначными контактами. Плата целиком одинакова для всех запусков.

```js
// Псевдокод порядка запусков; sourceA/loadA/portsB создаются на тестовом coupon.
const first = await base.sources(sourcesA).loads(loadsA).run();
// Здесь, после первого solve, объявляются порты цепи B.
const second = await base.sources(sourcesB).loads(loadsB).run();
const third = await base.sources(sourcesB).loads(raisedLoadsB).run();
```

Приёмка после JSON round-trip отчёта:

- Все meshReference разрешаются, не цикличны и принадлежат допустимому контексту/геометрии; ранний результат не мутировал.
- Для каждого `solution.domains[i]` доступен mesh domain: voltage length = nodes length; J/loss length = triangles length; индексы треугольников корректны.
- Второй результат рисует область B, а не остаётся серым при наличии computed field. Проверить также случай, когда ему нужны A+B.
- Повтор только RHS на той же B может переиспользовать подготовку/факторизацию/доступную геометрию; изменение контактов или сопротивления инвалидирует матрицу как раньше.
- Ранее возвращённые `voltageMap`/`lossMap` и ссылки plot остаются разрешимыми после всех запусков.

## 6. D3: сохраняемый валидатор реальных результатов

Перенести нужные assertions из `debugging/analysis-dsl/audit-dc-reexport.ts` в `tests/helpers/` или существующий test helper; отделить проверку данных от чтения файлов и HTML. Проверка принимает сериализованный report и test expectations, не выполняет физику и не ищет «последний успешный» output вместо текущего запуска.

Обязательные условия для стандартного четырёхрежимного real-board сценария:

1. Runner завершился успешно, есть четыре уникальных ожидаемых case IDs (две сетки × две нагрузки), все native solutions complete, нет failed sweep attempts. Отсутствующий случай не пропускается.
2. Каждое ожидаемое измерение существует, реально числовое и finite. Проверять cardinality/идентичность списка перед `every(...)`, чтобы пустой список не давал pass. Unknown не скрывается условным отображением.
3. Поля всех вычисленных domains конечны и согласованы с разрешённой геометрией; нет dangling mesh/plot references. Conditional для принятого snapshot допустим и сохраняется.
4. Quality guards соответствуют solver: residual ≤1e-8; KCL ≤1e-12 A **или** normalized KCL ≤1e-8; power error ≤1e-15 W **или** normalized power ≤1e-8. Проверять finite отдельно. Не вводить более мягкий критерий и не путать dimensioned floors разных величин.
5. Сходимость проверять для **каждого** наблюдаемого порта и обоих уровней нагрузки, сопоставляя случаи по mesh/load/measurement IDs, а не порядковым индексам `[0,2]`. Total conductor loss = copper + barrels; мощность внешней нагрузки не включается.
6. Сохранить текущий порог сходимости 2% и записать текущие floors отдельно: 1e-9 V для падения и 1e-9 W для потерь. При переносе не менять математический критерий незаметно. При nearly-zero coarse value floor предотвращает деление на ноль, а не означает, что значение стало известным.
7. USB: на каждой сетке проверка того же лимита падения 0.100 V проходит при 2 A и проваливается при 4 A; следующий loss check/plot существует. Не проверять только общее число `fail`: нужен конкретный case/check.
8. Thunder: в каждом нужном случае есть конечный ненулевой ток хотя бы одного реального межслойного сегмента; отсутствие barrel-результатов не заменяется пустым успешным списком.

Пример небольшой функции валидатора (в обычном Node/TS test helper, **не в DSL**):

```ts
function assertConverged(
  coarse: number, fine: number, absoluteFloor: number, name: string,
) {
  assert.ok(Number.isFinite(coarse) && Number.isFinite(fine), `${name}: finite`);
  const relativeChange = Math.abs(fine - coarse)
    / Math.max(Math.abs(coarse), absoluteFloor);
  assert.ok(relativeChange <= 0.02, `${name}: change=${relativeChange}`);
}

// Разные физические единицы — разные явные floors.
assertConverged(coarseDropV, fineDropV, 1e-9, "consumer drop [V]");
assertConverged(coarseCopperW + coarseBarrelW,
  fineCopperW + fineBarrelW, 1e-9, "conductor loss [W]");
```

Expectations могут описывать технические требования следующим образом; это пример формы, не обязательный новый framework:

```json
{
  "cases": ["mesh-1/load-1", "mesh-1/load-2", "mesh-0.5/load-1", "mesh-0.5/load-2"],
  "measurements": ["MCU", "CAN logic", "RS485"],
  "convergence": { "relative": 0.02, "voltageFloorV": 1e-9, "lossFloorW": 1e-9 },
  "requireFiniteMeasurements": true,
  "requireBarrelCurrent": false
}
```

В физические источники/нагрузки эта metadata не преобразуется. Runner вызывает audit текущего результата; при нарушении условий named test завершается ненулевым кодом, сохранив JSON/HTML/причину для разбора. Намеренный `a.check` fail по напряжению — ожидаемый исход сценария, не автоматический провал runner и не повод остановить DSL.

### Отрицательные тесты самого audit

Сделать маленький корректный test report, затем независимо испортить его копии. Каждый из следующих случаев обязан отклоняться:

- Из четырёх cases удалён один, дублирован ID или появились failed attempts.
- Нет ожидаемого потребителя; измерение null/unavailable/nonfinite; пустой массив вместо нужных измерений.
- Mesh reference отсутствует/зациклен; вычисленный domain не имеет geometry; неверная длина поля/индекс треугольника.
- Fine/coarse drop или conductor loss отличаются на 10% при допуске 2%.
- Плохой quality metric, который нарушает и абсолютный, и нормированный допуск.
- В USB нет ожидаемого raised-load fail либо следующей проверки/карты; в Thunder нет требуемого межслойного тока.

Корректный conditional snapshot с ожидаемым USB fail обязан приниматься. Такие тесты проверяют валидатор без повторного FEM; синтетические отчёты не считать доказательством физической точности.

## 7. Отчёт и контроль сохранения постановок

- Renderer использует реальные completed results, измерения/labels из существующих logs/checks и данные plot. Убрать зависимость от старого `SavedDcScenario` и сгенерированного `metadata.scenario`.
- Сохранить обзор восьми плат, карты слоя/режима, источники/возвраты, units/легенды, сравнение сеток и нагрузок, conditional и gray для uncomputed. Не маскировать missing geometry рендером только доступных domains.
- После переноса сценария сравнить **нормализованные физические входы** с прежним сохранённым запуском: контакты, источники/нагрузки, распределения, стек/материалы, сетки, ограничения. Исключать только incidental IDs/порядок эквивалентных записей/текст labels. Не ограничиваться числом источников.
- Исправление D1 может обоснованно изменить результат, если старый observation-port создавал электрод. В таком случае отдельно показать, какой контакт перестал быть активным и почему изменились числа; не удерживать ошибочную физику ради совпадения golden. Не менять токи/проводимость для компенсации.
- Старые captured JSON/source/evidence/manifest не редактировать. Generated logs, maps, screenshots — в ignored debugging; authored `.js`, expectations, helpers и tests — в git.
- Добавить hashes authored code, ресурсов/профилей, inputs и реализации в отчёт. Не использовать прежний hash как доказательство новой версии.

## 8. Проверки и условия приёмки

Исполнитель исправлений запускает focused DSL/DC и audit regression tests; typecheck/build; если меняется Rust — native build и соответствующие Rust DC/conduction regression tests; установленный package smoke проверяет production DSL→DC. Точные новые test filenames выбрать по существующему стилю.

После стабилизации выполнить `npm run test:pcb-analyze -- --all --name dc`: все восемь authored `dc.js` проходят настоящий production runner и сохраняемый audit, четыре случая на плату. Для неизменных старых результатов перенос audit можно сначала проверить offline; после изменения D1/D2 и authored-сценариев итоговое evidence должно относиться к новой версии. Не запускать дорогой корпус многократно без исправления/сбоя/нового существенного условия.

Visual QA: открыть новый отчёт STM32 с consumer table/checks, USB nominal/high load, Thunder с двумя слоями и синтетический D2 с новой областью B. Проверить реальные rendered карты, а не только наличие файлов. Сохранить screenshots и короткое описание проверенного; использовать одинаковую шкалу при сравнении нагрузок либо явно показывать auto-scale.

Передать на ревью:

- Матрицу D1/D2/D3/формат → исправление → regression → результат.
- Прямые сценарии восьми плат и отсутствие старого генератора/параллельной схемы.
- Результаты positive/negative audit-тестов, актуальную корпусную матрицу и hashes.
- Сравнение физических входов до/после миграции, объяснение изменений численных результатов D1.
- Пути к HTML, JSON, screenshots, командам и логам, оставшиеся ограничения.

Обновить README fixtures/анализатора, d.ts/документацию измерений при уточнении семантики, verification report и CHANGELOG. Не объявлять D1–D3 закрытыми лишь по просмотру кода; нужны соответствующие regression results. Существующие M1, strict/snapshot guards, бюджет/отмена и invalidation tests должны сохраниться.

**Готово:** исправлены D1/D2; audit воспроизводится из чистого checkout; пользователь читает и редактирует полноценный DSL прямо в `dc.js`; реальный корпус и визуальные отчёты проверены. Не создавать коммит автоматически и не начинать следующий анализатор — передать исправления на независимую проверку.
