# Analysis DSL: краткий синтаксис принятой основы

Дата: **2026-10-20**. Вариант B с builders согласован как рабочая основа. **Только документация, реализации нет.** Ранние объектные вызовы solver и физические параметры в analysis заменены формой ниже. Полный набор анализов: [справочник](dsl-reference.md); входы: [матрица](dsl-data-matrix.md).

```ts
const a = analysis({ board });
const components = [
  ic({ designator: "U1", ground: ["3", "4"], supplies: { core: ["1", "2"] } }),
];
const projectStackup = stackup({
  layers: [
    { kind: "copper", name: "TOP", thicknessMm: 0.035 },
    {
      kind: "dielectric", thicknessMm: 1.53,
      material: "FR4-project", relativePermittivity: 4.2, lossTangent: 0.02,
    },
    { kind: "copper", name: "BOTTOM", thicknessMm: 0.035 },
  ],
  viaPlatingThicknessUm: 25,
});
const input = a.port({ positive: pad("J1", "1"), negative: pad("J1", "2") });
const consumer = a.port({ positive: pad("U1", "1"), negative: pad("U1", "3") });
const power = {
  sources: [{ port: input, voltageV: 3.3, model: "ideal" }],
  loads: [{ port: consumer, currentA: 0.1 }],
};
const task = a.dc()
  .components(components)
  .stackup(projectStackup)
  .materials(models.materials.project)
  .sources(power.sources)
  .loads(power.loads)
  .temperatureC(25);

const preparation = await task.prepare(); // Необязательно; run тоже проверяет входы.
const dc = await task.run();
a.check({ name: "Питание U1", actual: dc.voltage(consumer), min: 3.1, unit: "V" });
a.plot({ title: "Потери в меди", data: dc.lossMap });
```

Значения учебные. Здесь намеренно задана точечная нагрузка между U1.1 и U1.3; списки supplies/ground сами по себе не распределяют её ток по остальным контактам. Для агрегированной нагрузки по сети уже принято equal-распределение питания и возврата с независимыми ручными токами; см. [правила](dsl-components-simple.md). Материалы должны быть предоставлены проектом.

Builder каждого анализа независим. Компоненты описываются массивом, режим — через sources/loads/heat/environment и другие предметные разделы. Один и тот же массив/stackup можно переиспользовать. Возвращённые builders неизменяемы; раздел заменяется целиком. Запуск — явный run().

Конструкторы физических описаний, check/plot и callback-операции не превращаются в builders без причины. Пример самостоятельного расчёта и подбора:

```ts
const a = analysis();
const calculateImpedance = async (widthMm) => {
  const line = a.line({
    kind: "microstrip", widthMm, copperThicknessMm: 0.035,
    dielectricHeightMm: 0.18, material: models.materials.laminate,
  });
  const result = await a.impedance()
    .target(line)
    .method("cross-section")
    .frequencyHz(1e9)
    .run();
  return result.characteristicImpedance.real();
};
const solution = await a.solve({
  parameter: { name: "widthMm", unit: "mm", min: 0.1, max: 0.6 },
  evaluate: calculateImpedance,
  target: 50, tolerance: 1, unit: "Ohm", maxEvaluations: 30,
});
```

check записывает pass/fail/unknown и не останавливает код. Ошибки подготовки/решателя — отдельные ошибки Promise, обрабатываемые try/catch. Неизвестное не равно нулю. Ручной список не дополняется; неразрешённая неопределённость отмечается в зависимых результатах и проверках.
