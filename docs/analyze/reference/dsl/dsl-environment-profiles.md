# DSL: профили среды и изменения температуры

Дата: **2026-10-20**. Согласовано направление: глобальный `profiles` и готовые сценарии холодной/горячей среды, колебаний и резких изменений. **Имена, значения и структуры ниже — предложение для обсуждения, не реализованный API.** [Полный справочник](dsl-reference.md).

## 1. Глобальный каталог

```ts
const cold = profiles.environments.cold;
const hot = profiles.environments.hot;
const custom = profiles.environment({
  name: "Мой режим",
  ambientC: 35,
});
```

`profiles` доступен в каждом сценарии без импорта. Каталог неизменяемый; `profiles.environment` создаёт отдельный профиль и не меняет встроенные. В отчёт записываются разрешённые значения и идентичность профиля, а не только имя `hot`.

| Предлагаемое имя | Температура среды | Назначение |
| --- | --- | --- |
| `room` | Постоянные +25 °C | Удобная исходная точка |
| `cold` | Постоянные −20 °C | Холодный запуск/работа |
| `hot` | Постоянные +70 °C | Работа в горячей среде |
| `oscillating` | Синусоида: среднее +25 °C, амплитуда 10 K, период 600 s | Плавные колебания |
| `rapidChange` | От −20 до +70 °C, начало на 60 s, переход за 5 s | Быстрая смена внешней температуры |
| `wideCycle` | −40 → +85 → −40 °C, переходы по 120 s и выдержки по 300 s | Повторяющиеся циклы с большой дельтой 125 K |

**Числа иллюстративные, не стандарт испытаний и не допустимые условия для любого компонента.** Значения надо сверять с задачей и диапазонами моделей. Профиль среды не определяет поток воздуха, коэффициент теплоотдачи, влажность или ресурс пайки. Механика/BER остаются отложенными.

## 2. Общая запись температуры

`ambientC` принимает число либо один из профилей времени:

```ts
type TemperatureC =
  | number
  | { kind: "step"; from: number; to: number; atS: number; riseS: number }
  | { kind: "sine"; offset: number; amplitude: number; periodS: number; phaseDeg: number }
  | {
      kind: "pwl";
      points: readonly (readonly [timeS: number, temperatureC: number])[];
      interpolation: "linear" | "hold";
      repeat?: { periodS: number; count: number };
    };
```

Это экземпляр общей формы waveform из справочника. `offset/from/to` имеют единицы °C, амплитуда — разность температур K; единицы определяются полем `ambientC`, дополнительно `unit` не требуется.

Точное предложение встроенных динамических профилей:

```ts
const oscillating = profiles.environment({
  name: "Колебания среды",
  ambientC: {
    kind: "sine",
    offset: 25,
    amplitude: 10,
    periodS: 600,
    phaseDeg: 0,
  },
});

const rapidChange = profiles.environment({
  name: "Быстрая смена среды",
  ambientC: { kind: "step", from: -20, to: 70, atS: 60, riseS: 5 },
});

const wideCycle = profiles.environment({
  name: "Большая дельта температур",
  ambientC: {
    kind: "pwl",
    points: [[0, -40], [300, -40], [420, 85], [720, 85], [840, -40]],
    interpolation: "linear",
    repeat: { periodS: 840, count: 10 },
  },
});
```

У `step` до `atS` действует `from`, за `riseS` значение линейно переходит к `to`, далее удерживается. `riseS: 0` — идеальный скачок граничного условия, не температуры всей платы.

Для PWL первая точка находится в 0, времена строго возрастают. Без repeat после последней точки значение удерживается. В цикле последняя точка находится в `periodS`; для непрерывного цикла начальное и конечное значения совпадают. После `count` циклов конечное значение удерживается. Горизонт расчёта задаётся отдельно; если он короче, моделируется только его часть, с записью этого в отчёте. Физические события waveform учитываются решателем независимо от шага вывода графика.

## 3. Использование в расчёте

```ts
const a = analysis();

const heat = await a.thermal()
  .network(thermalNetwork)
  .environment(profiles.environments.rapidChange)
  .initialC("ambient")
  .time({ stopS: 180, sampleStepS: 0.5 })
  .run();
```

`thermalNetwork` в этом фрагменте заранее описывает тепловые пути, ёмкости и мощности. `initialC: "ambient"` означает стартовую температуру, равную среде в момент t=0; это явное упрощённое начальное состояние, а не решение прогретого режима.

Возможны `initialC: 25`, `initial: previousHeat.finalState` или `initial: "steady"` для вычисления начального стационарного состояния при условиях t=0. Одновременно выбирается только один вариант. Несовместимое сохранённое состояние не переносится на другую сетку/геометрию молча.

Изменение воздуха не означает мгновенного изменения температуры меди и кристалла: их температуры вычисляются через тепловые пути и инерцию.

## 4. Профиль и охлаждение — разные вещи

```ts
const heat = await a.thermal()
  .stackup(profiles.stackups.project)
  .materials(models.materials.project)
  .environment(profiles.environments.hot)
  .components([
    device({ designator: "Q1", model: models.thermal.mosfet }),
  ])
  .heat([{ target: component("Q1"), node: "junction", powerW: 2 }])
  .cooling({
    boardSurfaces: profiles.cooling.boardNatural,
    heatsinks: [{
      target: component("Q1"),
      thermalNode: "case",
      model: models.thermal.heatsink,
      interface: models.thermal.interfacePad,
      cooling: models.thermal.heatsinkCooling,
    }],
  })
  .initialC(25)
  .time({ stopS: 600, sampleStepS: 1 })
  .run();
```

Это фрагмент сценария по плате. Ресурсы `models.thermal.*` и `profiles.cooling.boardNatural` должны быть предоставлены проектом и иметь подходящие условия применения; они не становятся достоверными встроенными значениями автоматически.

Среда задаётся через `.environment(...)` конкретного builder; в analysis общей среды нет. Профиль не меняет соседние расчёты или глобальный каталог. `.cooling(...)` описывает теплообмен, без второго ambientC. При повторном использовании builder новая environment возвращает новую конфигурацию, не изменяя исходную.

## 5. Сравнение режимов

```ts
const results = await a.sweep({
  parameter: {
    name: "environment",
    values: [
      profiles.environments.cold,
      profiles.environments.room,
      profiles.environments.hot,
    ],
  },
  evaluate: async (environment) => {
    const heat = await a.thermal()
  .network(thermalNetwork)
  .environment(environment)
  .initialC("ambient")
  .time({ stopS: 600, sampleStepS: 1 })
  .run();
    return heat.temperature("junction").max();
  },
});

a.plot({ title: "Максимальная температура по режимам", data: results.series });
```

Сценарий должен явно различать сравнение холодных стартов (`initialC: "ambient"`) и сравнение из одинакового начального состояния (`initialC: 25`).

Для DC, AC и иных статических расчётов температура материала задаётся отдельно через `temperatureC`; отсутствие этого значения не означает равенства температуре воздуха. Для самостоятельной оценки при температуре среды можно явно получить `profiles.sample({ environment, atS })` и передать полученное число. Такой расчёт предполагает равномерную температуру объекта и не заменяет thermal/electrothermal.
