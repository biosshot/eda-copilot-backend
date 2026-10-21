# Analysis DSL: полный рабочий справочник

**Базовый DSL согласован:** [решения по сценариям](dsl-scenario-decisions.md), [рабочий контракт](dsl-working-contract.md). Приняты channels([...]), drivers/receivers, port/ports, timing(options) у результата, a.model и операции измерений. Отдельный механизм контроля границ/двойного учёта моделей отклонён. Реализация не начата; изменения допускаются по обнаруженным ограничениям.

Дата: **2026-10-20**. **Проект интерфейса, без реализации.** Покрывает согласованные способы работы по плате и без платы. Это полный перечень пользовательских операций текущего плана, а не заявление о готовом решателе или поддержке произвольной физической модели.

**Согласованная основа:** вариант B — `analysis({ board })` хранит snapshot платы и отчёт; каждый анализ получает собственный builder с явным `.run()`. Компоненты, stackup, материалы и условия передаются выбранному анализу. Компоненты — массив описаний с `designator`. `stackup(...)` создаёт переиспользуемое описание стека. Это рабочая документация будущего API, не реализация.

**Подтверждено пользователем:** `check` регистрирует результат и не останавливает последующий код; `board`, `profiles`, `models` доступны глобально; нужны профили холодной/горячей среды и изменений температуры. Остальные формы ниже — конкретный черновик для обсуждения.

**Дополнительно согласовано:** нагрузка компонента на выбранной сети по умолчанию распределяется поровну между его контактами. Возврат — через точную сеть GND, также поровну. Ручные токи питания и возврата задаются независимо, остаток распределяется автоматически. [Полные правила](dsl-components-simple.md). Допущения обязательны в отчёте; обязательная семантическая детекция всех выводов не вводится.

[Связанные сценарии на плате компьютера и реальном CAD-экспорте](dsl-board-scenarios.md) показывают совместное использование источников, нагрузок, моделей и результатов.

При расхождении этот документ уточняет ранний [эскиз синтаксиса](dsl-syntax-draft.md). [Матрица данных](dsl-data-matrix.md), [профили среды](dsl-environment-profiles.md), [состав анализов](working-analysis-scope.md).

## 1. Общая форма программы и глобальные входы

```ts
const a = analysis({ board });
const dc = await a.dc()
  .sources(power.sources)
  .loads(power.loads)
  .temperatureC(70)
  .run();

a.check({ name: "Питание U1", actual: dc.voltage(consumer), min: 3.1, unit: "V" });
a.plot({ title: "Потери в меди", data: dc.lossMap });
```

`power` и `consumer` в этом кратком фрагменте задаются сценарием. Полные примеры ниже. Все примеры — JS/TS, не PHP; top-level await обозначает async-тело программы, точная оболочка runtime ещё не утверждена.

| Глобальное имя | Смысл |
| --- | --- |
| `board` | Неизменяемый snapshot существующего RoutingBoardV2. В запуске без платы — `undefined` |
| `profiles` | Неизменяемый каталог сценариев среды, stackup, охлаждения, правил и численных настроек; функции создания профилей |
| `models` | Неизменяемый каталог явно подключённых материалов и электрических/тепловых/EM-моделей |
| `analysis(options?)` | Создать контекст, принадлежащие ему расчётные объекты и отчёт |
| `pad(ref, number)` | Ссылка на вывод компонента; номер приводится к строке |
| `padId(id)` | Ссылка на конкретную площадку |
| `component(ref)` | Ссылка на компонент |
| `net(name)` | Выбор сети для области/проверок; не идеальный электрический узел |
| `region({ layers, polygon })` | Область в координатах платы, `polygon` использует существующий PolygonMm |

`analysis()` создаёт контекст **без платы**, даже если глобальный `board` существует. `analysis({ board })` явно требует плату; `board === undefined` в этой форме — ошибка. Сценарии, использующие только профиль/сечение/схему, не требуют CAD. Импорт нового формата платы и дублирование RoutingBoard здесь не вводятся.

Каталог `models` имеет разделы `materials`, `electrical`, `thermal`, `em`. `profiles` имеет `environments`, `stackups`, `cooling`, `checks`, `numerics`. Поддерживаемые базовые модели стандартных компонентов предлагается получать автоматически из данных проекта; сложные приборные модели и stackup требуют известных ресурсов или уточнения. Среда перед запуском показывает доступные ключи и метаданные; отсутствующий ресурс не заменяется похожим по имени.

`profiles.environment({name,ambientC})` создаёт собственный профиль, `profiles.sample({environment,atS})` возвращает температуру среды в °C в заданный момент. Встроенные режимы и их точная семантика описаны в [отдельном документе](dsl-environment-profiles.md).

### Контекст и независимые конфигурации

```ts
const a = analysis({ board });
const components = [
  capacitor({
    designator: "C1",
    model: models.electrical.decouplingCap,
    pins: { positive: "1", negative: "2" },
  }),
];

const task = a.dc()
  .components(components)
  .stackup(profiles.stackups.project)
  .materials(models.materials.project)
  .sources(power.sources)
  .loads(power.loads)
  .temperatureC(25);

const preparation = await task.prepare();
const dc = await task.run();
```

`analysis` принимает только необязательный board. Физические дополнения и режимы принадлежат конкретному builder. Ни компоненты, ни среда не наследуются из предыдущего анализа. Каждый вызов `a.dc()`/`a.thermal()` и аналогичных создаёт отдельную конфигурацию.

Builder неизменяемый: метод возвращает новую конфигурацию. Повторное задание раздела заменяет его целиком, а не объединяет скрыто. `.run()` фиксирует входы запуска; последующая мутация пользовательского массива/объекта не меняет запущенный расчёт. `await builder` ничего не запускает. `.prepare()` возвращает диагностику без численного решения; `.run()` выполняет ту же подготовку автоматически. При недостающих обязательных данных prepare показывает проблему, run отклоняет Promise до решения.

Методы группируют предметные разделы: `.time({ stopS, sampleStepS })`, а не отдельные setters каждой величины. Конструкторы описаний (`ic`, `stackup`, `a.line`, `a.circuit`, `a.thermalNetwork`) ничего не вычисляют. `check/plot/log/report` работают с отчётом; `sweep/solve/optimize` остаются объектными вызовами с callback `evaluate`.

Переданный `.stackup(...)` — явное расчётное описание стека, с записью заменённых CAD-значений в отчёте; исходная плата не меняется. Дублирование модели одного компонента внутри массива — ошибка. Физическая модель содержит свойства, роли и привязки; токи нагрузки, выделяемая мощность, охлаждение и начальное состояние задаются в сценарии.

### Конструктор стека

```ts
const projectStackup = stackup({
  viaPlatingThicknessUm: 25,
  layers: [
    { kind: "copper", name: "TOP", thicknessMm: 0.035 },
    {
      kind: "dielectric", name: "core", thicknessMm: 1.53,
      material: "FR4-project", relativePermittivity: 4.2, lossTangent: 0.02,
    },
    { kind: "copper", name: "BOTTOM", thicknessMm: 0.035 },
  ],
});
```

Числа иллюстративные. Слои перечислены сверху вниз, имена меди должны соответствовать плате. Форма близка к `stack({...})` Routing DSL, но этот конструктор возвращает описание, не изменяет CAD и не переносит routing-контекст. Внутри используется согласование с существующим RoutingStackup, не новая независимая модель платы.

Допускаются поля `boardThicknessMm`, `fallbackCopperThicknessOz`, `viaPlatingThicknessUm`, `solderMask` и слои с толщиной в mm либо oz для меди; противоречащие друг другу значения отклоняются. `boardThicknessMm`, если задан, должен быть согласован с суммой слоёв и принятой конвенцией маски; эта конвенция требует отдельной формализации. Ограничения маршрутизации вроде maxTrackWidthMm в конструктор физического стека не входят.

`material` ссылается на запись в переданном `.materials(...)` каталоге. Электрические и тепловые свойства требуются только выбранным методом. Явные локальные свойства слоя заменяют соответствующие библиотечные значения с записью происхождения. εr и lossTangent не заменяют теплопроводность и теплоёмкость. Отсутствующий надёжный CAD-stackup задаётся в DSL; профиль `profiles.stackups.project` должен быть предоставлен проектом, а не возникает автоматически.

## 2. Единицы, временные функции и общие опции

Числа с явными единицами в имени: `widthMm`, `currentA`, `powerW`, `stopS`, `startHz`. Ноль — известное нулевое значение; пропуск — неизвестно/не задано. NaN и бесконечность не являются физическими входами.

```ts
type Frequency = {
  startHz: number; stopHz: number; points: number; scale: "linear" | "log";
};
type Time = { stopS: number; sampleStepS: number };
type Waveform =
  | { kind: "step"; from: number; to: number; atS: number; riseS: number }
  | { kind: "sine"; offset: number; amplitude: number; periodS: number; phaseDeg: number }
  | {
      kind: "pulse"; low: number; high: number; delayS: number;
      riseS: number; highS: number; fallS: number; periodS: number; count: number;
    }
  | {
      kind: "pwl"; points: readonly (readonly [timeS: number, value: number])[];
      interpolation: "linear" | "hold"; repeat?: { periodS: number; count: number };
    };
```

Единица значений waveform определяется полем: `powerW`, `ambientC` либо `waveform` внутри явно токовой/напряженческой нагрузки/источника. У `pulse` `highS` — длительность верхней полки, период включает оба фронта и нижнюю полку; некорректная сумма длительностей отклоняется. После конечного числа импульсов остаётся `low`. Для step/PWL правила приведены в [профилях](dsl-environment-profiles.md).

Временной отсчёт каждого запуска начинается с 0. `sampleStepS` определяет вывод, а внутренний шаг выбирает решатель с учётом фронтов/точности. Значение `frequencyHz` вместо `frequency` допускается для одиночного частотного расчёта; оба одновременно запрещены.

Общие опции вычислений:

| Поле | Значение |
| --- | --- |
| `name?` | Имя запуска в отчёте |
| `environment?` | Среда данного теплового запуска; общей среды контекста нет |
| `temperatureC?` | Явная равномерная температура материалов для электрического расчёта; не температура воздуха |
| `numerics?` | Профиль допустимых ошибок/сходимости, поддерживаемый выбранным методом |
| `budget?` | `{ maxSeconds?, maxMemoryMiB? }`; превышение явно отмечается |
| `method?` | Именованный физический метод из таблицы ниже |
| Расчётное ядро | Собственная реализация; внешний backend через DSL не выбирается |

Профиль numerics задаёт ошибки/сходимость, а не формат хранения. Для нового анализатора принята F64 по умолчанию без ранней кампании F32; прежние правила F32 packer сохраняются. Это не изменение DSL, а уточнение принятой [численной политики](implementation-structure.md).

## 3. Проверки, журнал и графики

### `a.check` никогда не служит assert

```ts
const check = a.check({
  name: "Питание U1",
  actual: dc.voltage(consumer),
  min: 3.1,
  max: 3.6,
  unit: "V",
});

// Выполняется и при check.status === "fail".
a.plot({ title: "Потери в меди", data: dc.lossMap });
```

`check` возвращает запись `{ id, name, status, actual, expected, message }` и добавляет её в журнал/отчёт. Границы включительные. Режимы условия: `min/max` либо `target` вместе с `tolerance`; одновременно их не смешиваем.

| Ситуация | Запись | Выполнение следующей строки |
| --- | --- | --- |
| Условие выполнено | `pass`, информационная запись | Да |
| Значение вне допуска | `fail`, сообщение о провале | Да |
| Нет пригодных данных, NaN, результат не сошёлся, несовместимая единица | `unknown` + диагностика причины | Да |
| Ошибка параметров самой проверки: min > max, не задано условие | `unknown` + диагностика конфигурации | Да |

`check` **не бросает исключение, не отменяет задания и не меняет сам по себе код завершения сценария**. Итог отчёта отдельно показывает число провалов/неопределённых проверок. Внешний CI может отдельно оценивать этот отчёт; автоматический режим «упасть на fail» сейчас не вводится.

Это не обещание перехватить ошибку JS до вызова: например, обращение к несуществующей переменной в аргументе выполняется раньше `check`. Штатные accessor-методы результата при недоступной метрике возвращают значение с причиной отсутствия, чтобы `check` мог записать `unknown`; произвольные ошибки программы остаются ошибками.

Дополнительная форма для логического условия:

```ts
a.check({ name: "Нет нарушений геометрии", actual: geometry.violations.count(), equals: 0 });
```

`equals` применяется к boolean/строке/целому счётчику. Для физических вещественных величин задаются допуск/диапазон и единица. Для серии/карты выбирают явный агрегат: `actual: transient.voltage(consumer).min()`. Комплексный импеданс сначала преобразуют через `.real()` или `.magnitude()`.

### `a.log`, `a.plot`, `a.report`

```ts
a.log({ level: "info", message: "Расчёт завершён", data: { case: "hot" } });
a.plot({ title: "Питание", data: transient.voltage(consumer) });
a.plot({ title: "Температура платы", data: heat.temperatureMap, layer: "TOP", atS: 60 });
const report = a.report();
```

`log.level`: `info`, `warning`, `error`. Даже `error` — запись, а не команда остановить программу. `plot` выбирает представление по типизированным данным; опциональны `kind: "line" | "heatmap" | "table" | "eye" | "polar" | "bar"`, `layer`, `atS`, `atHz`. Непригодные данные дают запись с объяснением без изображения и без остановки сценария. Единицы, координаты и оси берутся из данных.

`report()` возвращает накопленные расчёты, проверки, журнал, графики и сводку. Это обычные структурированные результаты, доступные без GUI. Их сохранение/передача хостом не требует вводить в DSL доступ к файловой системе.

Хост получает накопленный отчёт и при завершении сценария без явного `report()`. При ошибке дальнейшего вычисления уже записанные проверки/графики сохраняются; ошибка выполнения и число fail-проверок — отдельные поля сводки.

Ошибка solver/API по умолчанию отклоняет соответствующий Promise, и её можно обработать обычным `try/catch`. Это отдельный случай от проваленной проверки. Независимые расчёты можно продолжить после обработки ошибки; непригодный результат не выдаётся за ноль.

## 4. Выбор объектов и построение задачи

| Метод | Основные поля | Что возвращает |
| --- | --- | --- |
| `a.port` | `{ positive, negative }` | Порт на конкретных площадках/контактах |
| `a.channel` | `{ from, to, via?, scope? }` | Канал с ветвями и существенным окружением |
| `a.differential` | `{ positive: channel, negative: channel }` | Дифференциальный канал |
| `a.line` | Геометрия самостоятельной линии | Линию без платы |
| `a.conductor` | `{ lengthMm, widthMm, thicknessMm, material }` | Прямой однородный проводник без платы |
| `a.circuit` | `{ nodes, ports, elements }` | Заданную электрическую сеть |
| `a.thermalNetwork` | `{ nodes, links, sources }` | Заданную тепловую сеть |
| `a.scene` | `{ model, parameters? }` | Самостоятельную параметризованную EM-сцену |
| `a.antenna` | `{ structure, feed, models? }` | Антенну из board либо EM-сцены |
| `a.model` | Именованную поддерживаемую модель и её параметры | Локальную модель без изменения глобального каталога |
| `a.variant` | `{ changes }` | Новый контекст с расчётными изменениями snapshot |

`positive/negative` порта допускают одну площадку или массив. Массив означает явно заданный общий внешний контакт, который может соединять выбранные площадки; это физическое допущение, отражаемое в отчёте. Имя сети не заменяет геометрию возврата. Неоднозначный `pad(ref, number)` требует выбора группы либо `padId`.

`via` канала — упорядоченные опорные селекторы для уточнения пути, а не команда добавить переходы. `scope` ограничивает выбор; backend обязан проверить достаточность окружения. Ветви и связанные проводники не исчезают при выборе двух концов.

### Автоматическое распознавание и ручные исключения

После замечания пользователя общий select/assign убран из основного интерфейса. Актуальное предложение — [стандартная автодетекция, builders исключений и отчёт подготовки](dsl-components-simple.md).

Подготовка конкретного анализа пытается распознать поддерживаемые компоненты по данным проекта. В `.components([...])` пользователь перечисляет только недостающие описания и явные уточнения: `ic({ designator: "U1", ... })`, `resistor({ designator: "R1", ... })`, `capacitor({ designator: "C1", ... })`. Пропущенные в массиве компоненты не исключаются из платы.

Указанное поле заменяется целиком; ручные списки выводов не дополняются автоматикой, в том числе вложенный объект supplies. Пропуск оставляет возможность разрешения из поддерживаемой модели/автоматики; пустой список — явное пустое значение. Для агрегированной нагрузки default — контакты компонента на выбранных rail/GND; ручные наборы и токовые overrides описаны в компонентном контракте. Найденные возможные пропуски ручного списка дают warning и условность зависимых результатов; математически некорректная постановка остаётся ошибкой.

Массовые значения задаются предметными defaults/overrides в loads, heat и других параметрах расчёта. Операция `prepare` и автоматическая подготовка каждого анализа показывают источник данных, готовность, допущения и отсутствующие обязательные сведения. Универсальные правила targets/rules/otherwise больше не предлагаются пользователю как основной способ описания задачи.

## 5. Модели и материалы

Ресурс `models.*` имеет идентичность, источник, диапазон применимости, поддерживаемые анализы, терминалы и параметры. Конкретные форматы IBIS/Touchstone/thermal tables загружает хост; доступ к файлам или загрузка модели по сети не выполняются скрыто.

Минимальные создаваемые модели:

| `a.model({ kind, ... })` | Обязательные/характерные поля |
| --- | --- |
| `"resistor"` | `resistanceOhm`, при температурной зависимости `referenceC`, `temperatureCoefficientPerK` |
| `"capacitor"` | `capacitanceF`, явно заданные `esrOhm`, `eslH` либо явно выбранный идеальный вариант |
| `"inductor"` | `inductanceH`, `seriesResistanceOhm` либо явно выбранный идеальный вариант |
| `"driver"` | `lowV`, `highV`, `outputResistanceOhm`; фронты заданы stimulus |
| `"receiver"` | `inputCapacitanceF`, `inputResistanceOhm`, `thresholdV`; это простая модель, не универсальный приёмник |
| `"thermal-path"` | Физическая Cauer-сеть: `nodes`, `links`; узлы корпуса/кристалла имеют определённый смысл |
| `"heatsink"` | `heatCapacityJPerK`, либо более подробная сеть модели |
| `"thermal-interface"` | `resistanceKPerW` либо модель с температурной зависимостью |
| `"cooling"` | `resistanceKPerW` или таблица `{ deltaTemperatureK, heatFlowW }` с условиями применения |

Для компонентов сложнее этих моделей используются предоставленные ресурсы, включая температурные/частотные таблицы и black-box multiport. Указываем поддерживаемые методы, не объявляем любой SPICE/IBIS-AMI файл автоматически совместимым.

Материалы задаются ресурсами с необходимыми полями: электропроводность, εr и диэлектрические потери; теплопроводность (при необходимости тензор), плотность и удельная теплоёмкость. Постоянные или таблицы имеют единицы/оси и допустимые диапазоны. Математическая схема аппроксимации таблиц остаётся задачей модели; выход за диапазон не скрывается.

## 6. Питание платы: DC, AC и transient

```ts
const a = analysis({ board });
const components = [
  capacitor({
    designator: "C1", model: models.electrical.decouplingCap,
    pins: { positive: "1", negative: "2" },
  }),
];
const input = a.port({ positive: pad("J1", "1"), negative: pad("J1", "2") });
const consumer = a.port({ positive: pad("U1", "5"), negative: pad("U1", "6") });
const power = {
  sources: [{ port: input, voltageV: 3.3, model: models.electrical.supply }],
  loads: [{
    port: consumer,
    currentA: 0.1,
    waveform: { kind: "step", from: 0.1, to: 1, atS: 1e-6, riseS: 20e-9 },
  }],
};

const dc = await a.dc()
  .components(components)
  .stackup(profiles.stackups.project)
  .materials(models.materials.project)
  .sources(power.sources)
  .loads(power.loads)
  .temperatureC(25)
  .run();
a.check({ name: "Питание U1", actual: dc.voltage(consumer), min: 3.1, unit: "V" });
a.plot({ title: "Потери в меди", data: dc.lossMap });

const ac = await a.pi()
  .components(components)
  .stackup(profiles.stackups.project)
  .materials(models.materials.project)
  .sources(power.sources)
  .loads(power.loads)
  .domain("frequency")
  .temperatureC(25)
  .frequency({ startHz: 1e3, stopHz: 1e8, points: 201, scale: "log" })
  .run();
const transient = await a.pi()
  .components(components)
  .stackup(profiles.stackups.project)
  .materials(models.materials.project)
  .sources(power.sources)
  .loads(power.loads)
  .domain("time")
  .temperatureC(25)
  .initial("steady")
  .time({ stopS: 10e-6, sampleStepS: 2e-9 })
  .run();
a.plot({ title: "Импеданс PDN", data: ac.impedance(consumer) });
a.plot({ title: "Провал питания", data: transient.voltage(consumer) });
```

`currentA`/`voltageV` задают рабочую точку, waveform — временное изменение с единицей содержащего поля. Источник обязан иметь `model` либо явное `"ideal"`. Допускается несколько источников и нагрузок. Для постоянной мощности/сопротивления нагрузка вместо `currentA` использует `powerW` или `resistanceOhm`, ровно один режим; постоянная мощность нелинейна и должна поддерживаться solver. Динамическая нагрузка может использовать `model`.

При time-расчёте `initial` выбирается явно: `"steady"` либо совместимое сохранённое `finalState`. Рабочая точка и начало waveform должны быть согласованы. AC — малосигнальное возмущение около рабочей точки; нельзя подставлять температурно/динамически несовместимую модель.

| Вызов | Варианты входа | Основные результаты |
| --- | --- | --- |
| `a.dc()` | Ровно один режим: board + sources/loads; circuit; conductor + currentA/voltageV | `voltage(port)`, `current(target)`, `losses`, `lossMap`, `voltageMap`, `currentDensityMap`; для conductor также `resistance` |
| `a.pi()`, frequency | board + sources/loads либо circuit; frequency, рабочая точка | `impedance(port)`, `transfer({from,to})`, резонансы |
| `a.pi()`, time | board + sources/loads либо circuit; time, initial | `voltage(port)`, `current(target)`, `losses`, `finalState` |

Карты доступны только там, где есть пространственная модель. Для circuit вместо карты доступны значения узлов/элементов. Возврат метрики, не предусмотренной моделью, даёт `unavailable`, не фиктивную карту.

Простейший DC-калькулятор без платы:

```ts
const a = analysis();
const conductor = a.conductor({
  lengthMm: 50, widthMm: 1, thicknessMm: 0.035,
  material: models.materials.copper,
});
const dc = await a.dc()
  .conductor(conductor)
  .currentA(2)
  .temperatureC(25)
  .run();
a.check({ name: "Сопротивление проводника", actual: dc.resistance, max: 0.03, unit: "Ohm" });
```

### Агрегированная нагрузка компонента

```ts
const loads = [{
  designator: "U1", rail: net("VCC"), currentA: 0.1,
  return: net("GND"), // По умолчанию именно эта сеть.
  // distribution и returnDistribution по умолчанию равны "equal".
  pinCurrentsA: { "1": 0.04 },
  returnPinCurrentsA: { "7": 0.06 },
}];
```

Для каждой стороны общий ток один и тот же: сначала вычитаются её ручные значения, затем остаток делится на число незаданных контактов этой стороны. Пять контактов питания дают 40/15/15/15/15 mA; три контакта возврата — 60/20/20 mA. Это два представления одного потока, не двойная нагрузка. Отсутствующий override означает автоматическую долю, явный 0 сохраняется.

Выбираются контакты указанного designator на rail/return; ground/supplies либо явные pins/returnPins ограничивают или явно заменяют множество, без скрытого дополнения. Ключ токового override вне выбранного множества — ошибка. Число неизвестных контактов, остаток и итоговые значения видны в отчёте. Условия и многодоменные случаи: [компоненты](dsl-components-simple.md). Прямая port-нагрузка остаётся альтернативой и не распределяется по другим контактам из-за наличия supplies.

### Заданная электрическая сеть без платы

```ts
const a = analysis();
const circuit = a.circuit({
  nodes: ["gnd", "src", "mid", "load"],
  ports: [
    { id: "input", positive: "src", negative: "gnd" },
    { id: "output", positive: "load", negative: "gnd" },
  ],
  elements: [
    { id: "V1", kind: "voltage-source", nodes: ["src", "gnd"], voltageV: 3.3, model: "ideal" },
    { id: "R1", kind: "resistor", nodes: ["src", "mid"], resistanceOhm: 0.05 },
    { id: "L1", kind: "inductor", nodes: ["mid", "load"], inductanceH: 10e-9, seriesResistanceOhm: 0 },
    { id: "C1", kind: "capacitor", nodes: ["load", "gnd"], capacitanceF: 10e-6, esrOhm: 0.02, eslH: 1e-9 },
    {
      id: "I1", kind: "current-source", nodes: ["load", "gnd"], currentA: 0.1,
      waveform: { kind: "step", from: 0.1, to: 1, atS: 1e-6, riseS: 20e-9 },
    },
  ],
});
const r = await a.pi()
  .circuit(circuit)
  .domain("time")
  .initial("steady")
  .time({ stopS: 10e-6, sampleStepS: 2e-9 })
  .run();
a.plot({ title: "Заданная RLC-сеть", data: r.voltage(circuit.port("output")) });
```

`nodes[0] → nodes[1]` задаёт положительное направление тока; у источника напряжения V = V(first) − V(second). Один reference-узел выбирается через `reference: "gnd"`; для примеров с именем `"gnd"` это документированное значение по умолчанию. В других схемах reference обязателен.

Виды элементов: resistor, capacitor, inductor, voltage-source, current-source и `kind: "model", model, pins: { modelTerminal: nodeName }`. Поддерживаются только заявленные модели, не произвольная netlist SPICE. У multiport электрические портовые пары и опорные сопротивления должны соответствовать ресурсу.

## 7. Thermal и электротепловой анализ

По плате builder `a.thermal()` получает физические описания через components/stackup/materials, источники тепла через heat/losses и условия через cooling/environment/initialC/initial/time. Хотя бы один источник тепла должен быть задан, либо явное `.heat([])` для пассивной реакции на изменение среды.

- `components`: массив описаний с designator и физическими моделями/thermalNodes, без powerW.
- `heat`: `{ target, node, powerW: number | Waveform }[]` либо предметные defaults/overrides; мощность принадлежит сценарию.
- `cooling.boardSurfaces`: профиль с физическим законом теплоотдачи/площадями или граничными температурами.
- `cooling.heatsinks`: `{ target, thermalNode, model, interface, cooling }[]`.
- Начальное состояние: `initialC: number | "ambient"` либо `initial: "steady" | finalState`, ровно одно.
- `domain: "steady"` вместо `time` запускает стационарный расчёт; `domain: "time"` требует time. При наличии time этот режим можно не писать.
- Динамическая среда в steady требует явного `atS`, фиксирующего граничное состояние. Это не история нагрева.

`environment` отвечает за температуру среды. Характеристика радиатора и теплообмен платы задаются отдельно; CFD не запускается. Потери из DC считаются постоянными, из transient — функцией времени. Повторное указание одной и той же физической мощности запрещается.

### Самостоятельная тепловая сеть и радиатор

```ts
const a = analysis();
const network = a.thermalNetwork({
  nodes: [
    { id: "ambient", boundary: "environment" },
    { id: "junction", heatCapacityJPerK: 0.1 },
    { id: "case", heatCapacityJPerK: 1 },
    { id: "sink", heatCapacityJPerK: 20 },
  ],
  links: [
    { from: "junction", to: "case", resistanceKPerW: 1.5 },
    { from: "case", to: "sink", resistanceKPerW: 0.5 },
    { from: "sink", to: "ambient", resistanceKPerW: 8 },
  ],
  sources: [{ node: "junction", powerW: 2 }],
});
const heat = await a.thermal()
  .network(network)
  .environment(profiles.environments.rapidChange)
  .initialC("ambient")
  .time({ stopS: 600, sampleStepS: 1 })
  .run();
a.check({ name: "Температура кристалла", actual: heat.temperature("junction").max(), max: 125, unit: "degC" });
a.plot({ title: "Нагрев кристалла", data: heat.temperature("junction") });
```

Числа учебные, не модель конкретного MOSFET или радиатора. `network` альтернативен board-источникам/cooling в этом вызове; все пути теплообмена здесь описаны links. У links вместо сопротивления можно задать совместимую модель теплопередачи из библиотеки.

Результат thermal: `temperature(nodeOrComponentNode)`, `temperatureMap`, `heatFlow(link)`, `finalState`, баланс энергии. Узел компонента выбирается `{ target: component("Q1"), node: "junction" }`. Карта не выдумывается для сети без геометрии.

### Двусторонний расчёт

```ts
const result = await a.electrothermal()
  .stackup(profiles.stackups.project)
  .materials(models.materials.project)
  .electrical({
    components: [
      device({
        designator: "Q1", model: models.electrical.mosfetElectrothermal,
        pins: { drain: "1", gate: "2", source: "3" },
      }),
    ],
    ...power,
    operating: [{ target: component("Q1"), gateVoltageV: 10 }],
  })
  .thermal({
    components: [device({ designator: "Q1", model: models.thermal.mosfet })],
    cooling,
    environment: profiles.environments.wideCycle,
    initialC: "ambient",
  })
  .couplings([
    { target: component("Q1"), thermalNode: "junction" },
  ])
  .time({ stopS: 8400, sampleStepS: 1 })
  .run();
```

Это фрагмент с заданными `power` и `cooling`; номера выводов условные. Результаты: `electrical`, `thermal`, `losses`, данные сходимости связи. Он пересчитывает температурно-зависимые электрические потери, а не просто передаёт один результат DC.

Связанный builder получает отдельные `.electrical({...})` и `.thermal({...})`, явные `.couplings([...])` и одну общую `.time({...})`. Физические stackup/materials общие для связанной платы. Вложенные разделы — обычные объекты конфигурации, не запущенные builders и не прежние результаты. Нельзя задать два независимых времени или две разные геометрии одной связанной платы.

Без платы задаются electrical.circuit, thermal.network и `couplings: [{ element: "R1", thermalNode: "junction" }]`. Coupling передаёт потери элемента в узел и температуру узла обратно в его электрическую модель. Источник этих же потерь не дублируется в thermal sources. Нужны температурные зависимости и совместимость моделей; универсальный нелинейный симулятор не подразумевается.

Искусственный самостоятельный пример двусторонней связи:

```ts
const a = analysis();
const circuit = a.circuit({
  nodes: ["gnd", "supply"],
  reference: "gnd",
  ports: [{ id: "supply", positive: "supply", negative: "gnd" }],
  elements: [
    { id: "V1", kind: "voltage-source", nodes: ["supply", "gnd"], voltageV: 5, model: "ideal" },
    {
      id: "R1", kind: "resistor", nodes: ["supply", "gnd"],
      resistanceOhm: 10, referenceC: 25, temperatureCoefficientPerK: 0.004,
    },
  ],
});
const network = a.thermalNetwork({
  nodes: [
    { id: "ambient", boundary: "environment" },
    { id: "resistor", heatCapacityJPerK: 2 },
  ],
  links: [{ from: "resistor", to: "ambient", resistanceKPerW: 10 }],
  sources: [],
});
const result = await a.electrothermal()
  .electrical({ circuit })
  .thermal({
    network, environment: profiles.environments.hot, initialC: "ambient",
  })
  .couplings([{ element: "R1", thermalNode: "resistor" }])
  .time({ stopS: 120, sampleStepS: 0.1 })
  .run();
a.plot({ title: "Температура резистора", data: result.thermal.temperature("resistor") });
```

## 8. Линии, импеданс, TDR, SI и crosstalk

`a.line` описывает самостоятельные поддерживаемые сечения:

| `kind` | Параметры |
| --- | --- |
| `"microstrip"` | `widthMm`, `copperThicknessMm`, `dielectricHeightMm`, `material`, `lengthMm?`, `solderMask?` |
| `"stripline"` | `widthMm`, `copperThicknessMm`, `topDielectricHeightMm`, `bottomDielectricHeightMm`, `topMaterial`, `bottomMaterial`, `lengthMm?` |
| `"coupled-microstrip"` | Параметры microstrip + `gapMm` для двух одинаковых линий |
| `"coupled-stripline"` | Параметры stripline + `gapMm` |

Высота диэлектрика измеряется от поверхности меди до поверхности опорной меди; `gapMm` — зазор между кромками. Это ограниченные сечения с явно описанными опорными проводниками, не универсальный 2D-CAD. Для другого сечения можно передать ресурс `models.em.crossSection` с поддерживаемой параметризацией.

| Метод | Вход | Метод/выход |
| --- | --- | --- |
| `a.impedance` | `target: line | channel | differential | crossSection`, частота | `method: "formula" | "cross-section" | "layered"`; Z0, RLGC, задержки, участки и сетевые характеристики там, где есть продольная модель |
| `a.tdr` | `target`, `referenceOhm`, `riseS`, `time`, терминация дальнего конца | Кривая отражения/импеданса по времени; расстояние только при заданной модели скорости |
| `a.si` | `channels` либо `network`, модели драйверов/приёмников, переключения, `time` | Сигналы, crosstalk; временные измерения через result.timing(options) |
| `a.eye` | Временной `signal`, `bitRateBps`, `discardS`, `phaseS`, `thresholdV` | Диаграмма, измеренные запасы по времени/напряжению; без BER |

Для TDR `termination` — явный `"matched"`, `"open"`, `"short"` либо модель; не следует незаметно считать приёмник идеальным согласованием. Для coupled/differential задаются опорные сопротивления и выбранный режим измерения.

`method: "formula"` ограничен поддерживаемыми сечениями и не используется как незаметный fallback для via. `"layered"` извлекает канал по плате. Full-wave запрашивается через `a.em`, результат сети затем может использоваться в SI/TDR.

Пример самостоятельной линии без платы:

```ts
const a = analysis();
const line = a.line({
  kind: "microstrip",
  widthMm: 0.3,
  copperThicknessMm: 0.035,
  dielectricHeightMm: 0.18,
  material: models.materials.laminate,
  lengthMm: 50,
});
const z = await a.impedance()
  .target(line)
  .method("cross-section")
  .frequencyHz(1e9)
  .run();
a.check({ name: "Импеданс", actual: z.characteristicImpedance.real(), min: 48, max: 52, unit: "Ohm" });
const tdr = await a.tdr()
  .target(line)
  .referenceOhm(50)
  .riseS(100e-12)
  .termination("matched")
  .time({ stopS: 10e-9, sampleStepS: 10e-12 })
  .run();
a.plot({ title: "TDR", data: tdr.impedance });
```

У line есть `line.port("input")` и `line.port("output")`; у coupled-line порты `positiveInput`, `negativeInput`, `positiveOutput`, `negativeOutput`. Длина обязательна для SI/TDR/EM линии, но не для локального Z0 сечения.

Пример SI по плате (контекст a содержит board, ресурсы моделей предоставлены проектом). Единственная форма активности — drivers/receivers; соседний переключающийся канал добавляется в ту же модель.

```ts
const tx = a.port({ positive: pad("U2", "3"), negative: pad("U2", "4") });
const rx = a.port({ positive: pad("U3", "7"), negative: pad("U3", "8") });
const channel = a.channel({ from: tx, to: rx });
const stimulus = {
  kind: "prbs", order: 7, seed: 1, bits: 4096,
  bitRateBps: 1e9, riseS: 100e-12, fallS: 100e-12,
};
const si = await a.si()
  .channels([channel, neighbourChannel])
  .stackup(profiles.stackups.project)
  .materials(models.materials.project)
  .drivers([
    { port: tx, model: models.electrical.driver, stimulus },
    { port: neighbourTx, model: models.electrical.driver, stimulus: { ...stimulus, seed: 2 } },
  ])
  .receivers([
    { port: rx, model: models.electrical.receiver },
    { port: neighbourRx, model: models.electrical.receiver },
  ])
  .time({ stopS: 4096e-9, sampleStepS: 10e-12 })
  .run();
const eye = await a.eye()
  .signal(si.voltage(rx))
  .bitRateBps(1e9)
  .discardS(20e-9)
  .phaseS(0)
  .thresholdV(1.65)
  .run();
a.plot({ title: "Eye", data: eye.diagram });
```

`neighbourChannel` задаётся тем же `a.channel` между предоставленными neighbourTx/neighbourRx. Отсутствие driver не удаляет медь или необходимую электрическую нагрузку. Ветки канала, земли, терминации и реальное окружение сохраняются.

Stimulus: PRBS как выше; `kind: "bits", bits: "010110...", bitRateBps, riseS, fallS, repeatCount`; либо `kind: "waveform", voltageV: Waveform` для явной формы. Seed должен быть ненулевым и допустимым для выбранного порядка; алгоритм PRBS и начальное состояние фиксируются для воспроизводимости. Драйвер задаёт уровни/выходное сопротивление; waveform задаёт напряжение на его входе по документированной модели, не дублирует источник в извлечённой меди.

`si.differentialVoltage({ positive: positivePort, negative: negativePort })` возвращает временную разность напряжений двух явно выбранных портов с совместимым reference. Именно эта серия передаётся в eye для дифференциального сигнала; форма accessor является проектом API.

SI без платы использует `channels: [line]` либо `network` с заданными портами. Для differential выбирается разность сигнальных напряжений, а опорные/возвратные контакты остаются в модели. `.channel(...)` у SI-builder исключён; конструктор a.channel сохраняется.

Для связанной линии/сети используется та же принятая форма drivers/receivers:

```ts
const si = await a.si()
  .network(coupledLine)
  .drivers([
    { port: coupledLine.port("positiveInput"), model: models.electrical.driver, stimulus: stimulusA },
    { port: coupledLine.port("negativeInput"), model: models.electrical.driver, stimulus: stimulusB },
  ])
  .receivers([
    { port: coupledLine.port("positiveOutput"), model: models.electrical.receiver },
    { port: coupledLine.port("negativeOutput"), model: models.electrical.receiver },
  ])
  .time({ stopS: 1e-6, sampleStepS: 10e-12 })
  .run();
```

`coupledLine` — результат `a.line({kind:"coupled-microstrip", ...})` с общей геометрией, материалами, длиной и зазором; stimuli заданы заранее. Две независимые standalone-линии не приобретают crosstalk без модели их взаимной связи. В board-режиме эту связь извлекают из общей геометрии каналов.

## 9. Геометрия и EMI

**Подтверждённое расширение:** `.excitations([...])` принимает электрические возбуждения на портах и предоставленные пространственные модели источников с placement и совместимым возбуждением. `.kind("coupling")` рассчитывает воздействие на наблюдаемые порты; `.measurement({ port, quantity: "voltage" })` задаёт наблюдение без неявной нагрузки; `emi.voltage(port)` возвращает напряжение. [Формы примеров и границы](dsl-scenario-decisions.md). Входные нагрузки задаются компонентами. Полученный сигнал не является автоматически выделенной помехой; необходимые данные и поддержка метода проверяются до расчёта.

```ts
const geometry = await a.geometry()
  .scope([net("+3V3"), net("GND")])
  .rules([
    { kind: "connectivity" },
    { kind: "clearance", minMm: 0.2 },
    { kind: "edge-clearance", minMm: 0.3 },
  ])
  .run();
a.check({ name: "Нет нарушений", actual: geometry.violations.count(), equals: 0 });
```

Геометрия требует board; без платы проверять нечего. `scope` опционален, по умолчанию вся плата. Дополнительные правила: `{kind:"length",channel,maxMm}`, `{kind:"skew",channels,maxMm}`, `{kind:"reference-continuity",channel}`, `{kind:"return-transition",channel,maxDistanceMm}`. Правила, требующие электрических допущений, маркируются отдельно от чисто геометрических измерений. Возвращаются violations/measurements с координатами и объектами, не только boolean.

EMI-риски:

```ts
const risks = await a.emi()
  .kind("risk")
  .signals([{ channel, riseS: 100e-12 }])
  .rules(profiles.checks.emi)
  .run();
```

`profiles.checks.emi` содержит явные пороги проверки возврата, размера петель и соседства с чувствительными объектами. Правило объясняет основание предупреждения. Без данных о токах/фронтах нельзя выдавать численный уровень излучения.

Кондуктивные помехи:

```ts
const conducted = await a.emi()
  .kind("conducted")
  .stackup(profiles.stackups.project)
  .materials(models.materials.project)
  .loads(power.loads)
  .sources(power.sources)
  .excitations([{ port: switchingPort, currentA: switchingWaveform }])
  .measurement({
    port: input,
    model: models.electrical.measurementNetwork,
    quantity: "voltage",
    detector: "rms",
    bandwidthHz: 9000,
  })
  .time({ stopS: 2e-3, sampleStepS: 10e-9 })
  .spectrum({
    discardS: 1e-3, window: "hann",
    startHz: 150e3, stopHz: 30e6,
  })
  .run();
```

Это фрагмент с заданными power/портами/формой тока; значения измерения иллюстративны, не настройка сертификации. Медь извлекается из board, источники и нагрузки задаются отдельно; альтернативно используется circuit без платы. Excitations добавляет только новые возбуждения, не дублируя уже включённые источники.

Альтернативная частотная постановка: вместо time/spectrum задаются `frequency` и `sources: [{port,currentSpectrum}]` либо `voltageSpectrum`; спектр — типизированные комплексные значения по частоте. Совместимость длительности/дискретизации/полосы измерения проверяется. Стартовый detector — RMS/peak в определённой полосе; quasi-peak и нормативные приёмники требуют отдельной поддержанной модели, не добавляются автоматически.

Результат: спектры напряжения/тока в измерительном порту, временные сигналы при transient и сведения о способе измерения. Геометрические риски без платы отсутствуют, но кондуктивный расчёт по заданной circuit возможен.

## 10. Full-wave, печатные/SMD-антенны и излучение

```ts
const detailed = await a.em()
  .target(channel)
  .stackup(profiles.stackups.project)
  .materials(models.materials.project)
  .method("full-wave")
  .frequency({ startHz: 1e8, stopHz: 5e9, points: 401, scale: "linear" })
  .space({ kind: "free-space" })
  .outputs(["sParameters", "fields"])
  .run();
```

В `a.em` поле `space` описывает электромагнитное окружение, а `environment` в остальных расчётах сохраняет единственный смысл — тепловой профиль. Это уточняет ранний эскиз, где свободное пространство называлось environment. Температура материалов EM при необходимости задаётся `temperatureC`; температура воздуха не превращается автоматически в температуру материала.

Target: channel, differential, line с длиной, antenna, EM-scene. Значение `"full-wave"` требует поддержанного собственного метода; никаких молчаливых замен. Автоматически подготовленная область должна сохранять существенные возвратные пути; параметры сетки/границ и сходимость остаются видимыми в diagnostics.

`outputs`: `sParameters`, `fields`, `radiation`. Получение S-матрицы использует нормированные возбуждения портов по очереди; поле или излучение для конкретного режима требуют `excitations`. Один порт допускает сокращённое `excitation`.

```ts
const antenna = a.antenna({ structure: board, feed: antennaPort });
const radiation = await a.em()
  .target(antenna)
  .stackup(profiles.stackups.project)
  .materials(models.materials.project)
  .method("full-wave")
  .frequency({ startHz: 2.3e9, stopHz: 2.6e9, points: 151, scale: "linear" })
  .space({ kind: "free-space" })
  .excitation({ port: antennaPort, incidentPowerW: 1, phaseDeg: 0 })
  .observation({
    frequenciesHz: [2.45e9],
    thetaDeg: { from: 0, to: 180, step: 2 },
    phiDeg: { from: 0, to: 360, step: 2 },
    distanceM: 3,
  })
  .outputs(["sParameters", "radiation"])
  .run();
a.plot({ title: "Диаграмма направленности", data: radiation.pattern, atHz: 2.45e9 });
```

`antennaPort` — выбранный feed с возвратом. Для PCB-излучения без антенны `target: board` и `excitations: [{port,voltageSpectrum}, ...]`/`currentSpectrum` описывают реальные когерентные источники. Для комплексного спектра задана фаза; некогерентные источники требуют явно выбранного статистического правила и не суммируются произвольно.

Для SMD-антенны model-привязка с designator и pins передаётся в `.components([...])` конкретного EM-builder. Для произвольной детали используется `device({ designator: "ANT1", model: models.em.smdAntenna, pins })`; ic предназначен для микросхем. Ресурс должен иметь пространственную EM-информацию для излучения/взаимодействия с платой. Один S11 не восстанавливает поле и диаграмму.

Без PCB:

```ts
const a = analysis();
const scene = a.scene({
  model: models.em.patchScene,
  parameters: { patchWidthMm: 32, patchLengthMm: 40 },
});
const antenna = a.antenna({ structure: scene, feed: scene.port("feed") });
```

`models.em.patchScene` содержит геометрию, материалы, порты и допустимые параметры. Это явно предоставленная модель, не автоматически существующий шаблон. Низкоуровневый универсальный язык 3D-геометрии сейчас не вводится; новые формы задаются ресурсом сцены или данными существующего board. Это позволяет выразить standalone EM-задачу, не обещая поддержку любой геометрии из одного footprint.

Корпус устройства и кабели исключены из текущего состава. Излучение относится к смоделированной структуре и возбуждению; расстояние и допустимость приближения дальнего поля проверяются. Оно не является оценкой полного EMC-соответствия устройства.

## 11. Сетевые модели и S-параметры

Предлагаемый служебный API для согласованных SI/PI/EM:

- `a.network({ model, ports })` — привязка предоставленной multiport-модели к именованным портам.
- `a.connect({ networks, connections, ports })` — составить сеть; `connections` явно сопоставляют порты/возвраты, внешние порты перечислены отдельно.
- `a.networkParameters().network(network).frequency(frequency).representation("S").referenceOhm(50).run()` — получить параметры; representation также допускает Z/Y.
- `a.tdr().target(network)` и `a.si().network(network)` — конфигурации для сетевой модели; после задания остальных условий запускаются через run(). Диапазон/причинность/пассивность проверяются там, где нужны.

`ports` в `a.network` сопоставляет имена DSL именам портов ресурса, например `{ input: "port1", output: "port2" }`. После создания доступны `network.port("input")`/`network.port("output")`. В `a.connect` соединение задаётся `{ from: left.port("output"), to: right.port("input") }`, внешние порты — именованной картой handles. Подключение соединяет определённые в модели пары контактов, включая возврат, а не только номер сигнального узла.

```ts
const a = analysis();
const network = a.network({
  model: models.electrical.channel,
  ports: { input: "port1", output: "port2" },
});
const parameters = await a.networkParameters()
  .network(network)
  .frequency({ startHz: 1e6, stopHz: 1e9, points: 201, scale: "log" })
  .representation("S")
  .referenceOhm(50)
  .run();
a.plot({
  title: "Передача канала",
  data: parameters.parameter({ out: "output", in: "input" }).magnitude(),
});
```

`networkParameters` принимает `referenceOhm` как общее значение либо карту по именам портов. Результат даёт `parameter({out:"output",in:"input"})` — комплексную серию; для S это соответствующее Sij. Само преобразование параметров не расширяет исходный частотный диапазон модели.

Внутреннее представление сети и формат ресурса не фиксируются новым board-контрактом. Порты модели — не просто номера контактов без reference. Соединение не добавляет автоматически паразитики отсутствующей геометрии. Универсальный RF-инструментарий сверх потребностей SI/PI/EM этим предложением не объявляется обязательным.

## 12. Sweep, подбор, оптимизация и варианты платы

Автоматический подбор номиналов, количества и размещения развязывающих конденсаторов не входит в обязательную первую версию. Проверка заданной конфигурации и общий sweep явно заданных вариантов сохраняются.

```ts
const sweep = await a.sweep({
  parameter: { name: "powerW", unit: "W", values: [1, 2, 3] },
  evaluate: async (powerW) => {
    const heat = await a.thermal()
  .network(makeThermalNetwork(powerW))
  .environment(profiles.environments.hot)
  .initialC("ambient")
  .time({ stopS: 600, sampleStepS: 1 })
  .run();
    return heat.temperature("junction").max();
  },
});
a.plot({ title: "Температура от мощности", data: sweep.series });
```

`makeThermalNetwork` — обычная функция пользователя, возвращающая a.thermalNetwork с заданной мощностью. Параметр может быть числом с unit, моделью или профилем без unit. `evaluate` возвращает скаляр либо объект именованных метрик. Несколько переменных: `parameters: { widthMm: {unit:"mm",values:[...]}, gapMm: {...} }`; callback получает объект. Режим сетки `mode: "cartesian"` или `"zip"` задаётся явно.

Подбор одной величины:

```ts
const solution = await a.solve({
  parameter: { name: "widthMm", unit: "mm", min: 0.1, max: 0.6 },
  evaluate: calculateImpedance,
  target: 50,
  tolerance: 1,
  unit: "Ohm",
  maxEvaluations: 30,
});
```

Callback возвращает физический скаляр с единицей. Статусы: `found`, `not-found`, `budget-exhausted`, `failed`; `not-found` означает отсутствие найденного решения в исследованной области/бюджете, а не доказательство невозможности.

Общая оптимизация:

```ts
const best = await a.optimize({
  parameters: {
    widthMm: { unit: "mm", min: 0.1, max: 0.6 },
    gapMm: { unit: "mm", values: [0.15, 0.2, 0.3] },
  },
  evaluate: calculatePairMetrics,
  objective: { metric: "loss", direction: "minimize", unit: "dB" },
  constraints: [
    { metric: "impedance", min: 95, max: 105, unit: "Ohm" },
  ],
  maxEvaluations: 100,
  seed: 1,
});
```

`calculatePairMetrics` возвращает `{ loss, impedance }` как именованные скаляры. Метод/глобальная оптимальность не обещаются; результат хранит лучшие найденные допустимые кандидаты, историю и нарушения ограничений. Это проект интерфейса под согласованный подбор, выбор конкретных алгоритмов остаётся отдельным.

В `sweep/solve/optimize` ошибка одного callback записывается в попытку, остальные продолжаются до бюджета; пользовательская отмена останавливает исследование. `check` внутри callback остаётся записью и **не** становится автоматически ограничением оптимизации. Для ограничения используется `constraints`. Непригодные метрики не улучшают objective.

Вариант платы:

```ts
const wider = a.variant({
  changes: [
    { kind: "track-width", target: net("+3V3"), widthMm: 0.8 },
  ],
});
const candidate = await wider.dc()
  .components(components)
  .stackup(profiles.stackups.project)
  .materials(models.materials.project)
  .sources(power.sources)
  .loads(power.loads)
  .temperatureC(25)
  .run();
```

Дополнительные changes: `stackup`, `material`, `component-model`, `component-pose` с координатами/поворотом. Это изменения расчётного snapshot. Явные физические параметры builder имеют приоритет над snapshot с записью замены; при исследовании нового stackup/model нельзя повторно передать старое переопределение и ожидать действия варианта. При изменении размеров/расположения нужны повторная проверка геометрии, обновление контактов и зависимого извлечения; для Pour — корректная перезаливка при доступных полных правилах. Если перестроение невозможно, вариант непригоден и не считается по старой меди. Перемещение footprint не делает автотрассировку и не восстанавливает разорванные соединения молча.

Ссылки на неизменившиеся контакты переопределяются по стабильной идентичности в новом snapshot. Несовместимые handles/результаты отклоняются. Исходная CAD-плата не изменяется; API применения варианта в редактор — за пределами Analysis DSL.

## 13. Контракт результатов и отчёта

Каждый результат имеет `id`, `kind`, `status: "complete" | "partial"`, `diagnostics`, `method`, `inputs`, `assumptions`. Полная невозможность расчёта — отдельная ошибка Promise; partial допустим только при явно известной области пригодности и не приравнивается к complete.

Скаляр: `{ value, unit, validity }`; временная/частотная серия: ось + единицы + значения + область валидности; карта: координаты/слои/время + единица + связь со snapshot. `validity` различает пригодное, условное значение и unavailable с причиной. Неизвестное значение не хранится как ноль. Неразрешённая неопределённость, существенная для конкретного check, даёт unknown; принятое и применимое приближение само по себе не запрещает проверять результат внутри его заявленной области.

Отчёт подготовки содержит фактические значения, source каждого поля, заменённые значения, кандидаты/конфликты, применённые приближения и готовность именно выбранного анализа. Ручной неполный список не дополняется; предупреждение связывается с зависимыми метриками. Разрешимый расчёт может продолжаться с явно ограниченной пригодностью. Неопределённость не относящихся к этому анализу сведений его не блокирует.

Операции данных: `.min()`, `.max()`, `.abs()`, `.at({ timeS })` / `.at({ frequencyHz })`, `.real()`, `.imaginary()`, `.magnitude()`, `.phaseDeg()`. Правила приняты в [рабочем контракте](dsl-working-contract.md). Они применяются только к подходящим типам; недопустимая операция даёт unavailable и диагностику. Редукция по частично неизвестным данным по умолчанию не скрывает пропуски. `plot` и `check` объясняют, почему данные непригодны.

Численные метрики сохраняются независимо от графиков. Проверка max по дискретным отсчётам относится к этим отсчётам и не автоматически доказывает ограничение между ними. Шаг, интерполяция и оценка достаточности разрешения отражаются в отчёте.

## 14. Покрытие согласованного состава

| Возможность | Вызовы | Без платы |
| --- | --- | --- |
| Геометрия | geometry | Нет; нужна геометрия board |
| DC по реальной меди | dc().sources(...).loads(...).run() | conductor или circuit для заданной модели |
| Thermal стационарно/во времени, радиатор | thermal | thermalNetwork |
| Электротепловой | electrothermal | circuit + thermalNetwork + couplings |
| Импеданс, частотные характеристики, TDR | line/channel → impedance/tdr | line/crossSection/network |
| SI/crosstalk | si | line/coupled-line/network |
| Временные измерения SI | result.timing(options), sampling/propagation | Да, по результату SI |
| Eye | eye | Да, по временному сигналу |
| AC/transient PI | pi | circuit |
| EMI-риски | emi risk | Нет; board-level правила |
| Кондуктивные помехи | emi conducted | circuit + источники + измерительная модель |
| Full-wave линий/антенн и излучение | em | scene/line + подходящий backend |
| S-параметры для перечисленных задач | network/networkParameters/connect | Да |
| Sweep/подбор | sweep/solve/optimize | Да |
| Проверки, графики, журнал | check/plot/log/report | Да |

BER, механика, CFD, корпус устройства/кабели, универсальный nonlinear-SPICE/IBIS-AMI не добавляются этим справочником в текущий состав.

Все операции определены на уровне пользовательского API. Проверка совместимости физических моделей, библиотечные форматы, механизмы исполнения и точность остаются предметом следующего проектирования. Полный синтаксис не означает, что рабочий RoutingBoardV2 уже обеспечивает все необходимые физические данные.

## 15. Что осталось после предварительного согласования DSL

Порядок реализации и проверки этих вопросов теперь определён в [плане P0–P15](implementation-plan.md). Достаточность входного board проверена статически в [аудите V2](routing-board-v2-sufficiency.md); выявленные пробелы закрываются до соответствующих расчётных этапов. Это не повторное открытие уже согласованного DSL.

Состав анализов и общая форма сценария уже выбраны. Не требуется заново выбирать язык, репозиторий или переделывать DSL целиком.

Пользовательские вопросы из [аудита DSL](dsl-readiness-review.md) закрыты принятой рабочей редакцией либо явно исключены пользователем. Таблица ниже относится к дальнейшей формализации и реализации, а не к необходимости заново согласовывать базовый DSL.

| Порядок обсуждения | Открытое решение | Конкретный результат обсуждения |
| --- | --- | --- |
| 1 | Компоненты, модели и автоматическое обнаружение | Откуда берём роли выводов, домены питания, потребление/мощности; как сопоставляем модели; как разрешаем несколько шин/контактов и неизвестные данные |
| 2 | Полнота физического входа | Перепроверка RoutingBoardV2, native Pour/Fill, площадок/vias, материалов и stackup по матрице; необходимые дополнения без дублирования типов |
| 3 | Поддерживаемая физика и границы каждого метода | Какие структуры/частоты/модели поддерживаются, что считается приближением; собственный EM и условия сходимости |
| 4 | Формальный контракт результатов | Точные типы чисел/серий/карт, validity/partial, единицы, diagnostics, происхождение назначений и привязка к PCB |
| 5 | Исполнение | Разрешённый JS/TS, лимиты, отмена/прогресс, кэширование, зависимые расчёты, Rust/Node граница и численная политика |
| 6 | Совместимость с текущими системами | Переиспользование Routing DSL требований, выдача карт в Preview, миграция RawPcb/Router и локализация CAD-адаптера |
| 7 | Проверка и последовательность реализации | Эталонные задачи/измерения, критерии точности и производительности, порядок создания общих решателей и итоговая перепроверка всего плана |

Равномерная нагрузка и ручные токи питания/возврата уже приняты. Дальше требуется проверить достаточность данных на платах, многопортовые/корпусные модели, динамические overrides и формы пространственных couplings. Распределение по площади — кандидат явного режима, не новый default. Реализация не начата.
