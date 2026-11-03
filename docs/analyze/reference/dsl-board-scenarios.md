# Analysis DSL на плате компьютера и на существующем CAD-экспорте

Дата: **2026-10-20**. Проект интерфейса, **без реализации и без результатов симуляции**. [Справочник](dsl-reference.md), [нагрузки и ручные токи](dsl-components-simple.md).

## 1. Что именно моделируем

Главный пример — связанный сценарий для **условной платы одноплатного компьютера класса Raspberry Pi**: вход 5 V, преобразователь питания, SoC, RAM, USB и Ethernet, несколько шин, дифференциальная линия и радиатор. Это не модель конкретной Raspberry Pi: обозначения, номера контактов, stackup, токи, тепловые параметры и пределы ниже учебные. Нельзя подставлять их в настоящую Raspberry Pi без сверки проекта.

Официальная [сокращённая схема Raspberry Pi 4](https://datasheets.raspberrypi.com/rpi4/raspberry-pi-4-reduced-schematics.pdf) показывает контекст реального компьютера, но не предоставляет в этом примере полный RoutingBoard с физической медью. Для расчёта геометрии нужен фактический CAD-snapshot. Поэтому ниже отдельно приведён пример с **проверенными контактами существующего экспорта STM32/CAN/RS-485 из репозитория**.

Все разделы 2–9 используют одну плату, одни обозначения и общие переменные; они не являются независимыми сниппетами с несовместимыми номерами. Код читается последовательно как async-программа. Хост предоставляет board, models и profiles. Ошибка одной симуляции по обычным правилам JS требует try/catch, если сценарий должен продолжить остальные.

### Условная карта платы SBC

| Объект | Контакты и назначение в примере |
| --- | --- |
| J1 | 1 — VIN_5V, 2 — GND |
| U5: PMIC | 1 — VIN_5V, 2 — GND; выходные границы 10 — VCORE, 11 — VIO_3V3, 12 — VDDR_1V8 |
| U1: SoC | B1/B2/C1/C2 — VCORE; D1/D2 — VIO_3V3; A1/A2/A3/A4 — GND |
| U2: RAM | 1/2 — VDDR_1V8; 3/4 — GND |
| U3: USB-контроллер | 1 — VIO_3V3, 2 — GND, 3 — USB_DP, 4 — USB_DM |
| U4: Ethernet-контроллер | 1 — VIO_3V3, 2 — GND |
| J2: USB-разъём | 3 — USB_DP, 2 — USB_DM, 4 — GND |

Остальные пассивные компоненты и геометрия существуют в board. Выбор источника/канала не удаляет окружающую медь, ответвления и развязку. Неполная модель детали, существенной для анализа, остаётся видимой проблемой подготовки.

## 2. Общие данные: плата, стек и модели

```ts
const a = analysis({ board });

const projectStackup = stackup({
  viaPlatingThicknessUm: 25,
  layers: [
    { kind: "copper", name: "TOP", thicknessMm: 0.035 },
    { kind: "dielectric", name: "prepreg-top", thicknessMm: 0.18,
      material: "FR4-project", relativePermittivity: 4.2, lossTangent: 0.02 },
    { kind: "copper", name: "INNER1", thicknessMm: 0.035 },
    { kind: "dielectric", name: "core", thicknessMm: 1.10,
      material: "FR4-project", relativePermittivity: 4.2, lossTangent: 0.02 },
    { kind: "copper", name: "INNER2", thicknessMm: 0.035 },
    { kind: "dielectric", name: "prepreg-bottom", thicknessMm: 0.18,
      material: "FR4-project", relativePermittivity: 4.2, lossTangent: 0.02 },
    { kind: "copper", name: "BOTTOM", thicknessMm: 0.035 },
  ],
});
const materials = models.materials.sbc;

const electricalComponents = [
  ic({ designator: "U1" }),
  ic({ designator: "U2" }),
  ic({ designator: "U3" }),
  ic({ designator: "U4" }),
  device({ designator: "U5", model: models.electrical.sbc.pmicBoundary }),
];
const thermalComponents = [
  ic({ designator: "U1", model: models.thermal.sbc.soc }),
  ic({ designator: "U2", model: models.thermal.sbc.ram }),
  ic({ designator: "U3", model: models.thermal.sbc.usb }),
  ic({ designator: "U4", model: models.thermal.sbc.ethernet }),
  device({ designator: "U5", model: models.thermal.sbc.pmic }),
];
```

Это учебный четырёхслойный стек суммарной толщиной 1.6 mm без маски. Имена слоёв должны соответствовать конкретному snapshot. Каталог materials содержит проводимость меди, а также теплопроводность, плотность и теплоёмкость материалов. Значение εr не заменяет их.

Записи ic без ролей выводов достаточны для принятой агрегированной нагрузки; они не дают модели внутренней динамики SoC. Категория может быть известна из CAD и тогда даже эти уточнения не нужны. Поддерживаемые R/C/L обнаруживаются автоматически; для PI нужны их реальные паразитные параметры или явно выбранные модели.

### Важная граница модели PMIC

В этом примере источники на выходах PMIC — **границы расчёта разводки шин**, а не полноценная симуляция преобразования 5 V в низкие напряжения. Ресурс pmicBoundary явно описывает отсутствие передачи энергии между входом и выходами внутри извлечённой задачи; ни индуктивности, ни нелинейная модель конвертера не появляются из одного footprint. Источники на выходных границах не должны дублироваться второй активной моделью PMIC.

Входное потребление U5 задаётся отдельным сценарием. Его нельзя автоматически прибавить к выходным мощностям для оценки потребления устройства: это разные сечения одной системы. Для энергетически связанного PMIC нужен отдельный поддерживаемый многопортовый/преобразовательный ресурс вместо boundary-режима. Равномерная нагрузка сама такой моделью не является.

## 3. Напряжения, потребление по шинам и DC

```ts
const input5V = a.port({ positive: pad("J1", "1"), negative: pad("J1", "2") });
const coreSource = a.port({ positive: pad("U5", "10"), negative: pad("U5", "2") });
const ioSource = a.port({ positive: pad("U5", "11"), negative: pad("U5", "2") });
const ddrSource = a.port({ positive: pad("U5", "12"), negative: pad("U5", "2") });
const coreProbe = a.port({ positive: pad("U1", "B1"), negative: pad("U1", "A1") });

const sources = [
  { port: input5V, voltageV: 5.0, model: models.electrical.sbc.inputSupply },
  { port: coreSource, voltageV: 1.1, model: models.electrical.sbc.coreSupply },
  { port: ioSource, voltageV: 3.3, model: models.electrical.sbc.ioSupply },
  { port: ddrSource, voltageV: 1.8, model: models.electrical.sbc.ddrSupply },
];

const coreLoad = {
  designator: "U1", rail: net("VCORE"), currentA: 2.0,
  // GND, equal и returnDistribution: equal действуют по умолчанию.
  pinCurrentsA: { B1: 0.8 },
  returnPinCurrentsA: { A1: 0.8 },
};
const otherLoads = [
  { designator: "U1", rail: net("VIO_3V3"), currentA: 0.12 },
  { designator: "U2", rail: net("VDDR_1V8"), currentA: 0.45 },
  { designator: "U3", rail: net("VIO_3V3"), currentA: 0.20 },
  { designator: "U4", rail: net("VIO_3V3"), currentA: 0.25 },
  { designator: "U5", rail: net("VIN_5V"), currentA: 1.2 },
];
const loads = [coreLoad, ...otherLoads];

const dcTask = a.dc()
  .components(electricalComponents)
  .stackup(projectStackup)
  .materials(materials)
  .sources(sources)
  .loads(loads)
  .temperatureC(25);

const preparation = await dcTask.prepare();
a.plot({ title: "Входные данные и допущения DC", kind: "table", data: preparation.components });
const dc = await dcTask.run();
a.check({ name: "VCORE у B1/A1", actual: dc.voltage(coreProbe), min: 1.045, unit: "V" });
a.plot({ title: "Потери в меди платы", data: dc.lossMap });
a.plot({ title: "Плотность тока", data: dc.currentDensityMap });
```

У U1 для VCORE остаются три контакта: каждый получает (2.0 − 0.8) / 3 = 0.4 A. Точно так же три незаданных контакта возврата получают по 0.4 A. Вторая нагрузка U1 на VIO добавляет 0.12 / 4 = 0.03 A возврата на каждый GND. Итого A1 возвращает 0.83 A, остальные — по 0.43 A. В отчёте сохраняется вклад каждого домена; два определения на разных шинах не являются дубликатом нагрузки.

Порог 1.045 V учебный. Проверка измеряет именно пару B1/A1, а не гарантирует напряжение на всех выводах SoC. Источник задан на реальной площадке U5, поэтому падение по пути до U1 не исчезает. Физические GND также имеют сопротивление.

Возможное уточнение контактного набора без объявления всех ролей микросхемы:

```ts
const restrictedCoreLoad = {
  ...coreLoad,
  pins: ["B1", "B2", "C1"],
  returnPins: ["A1", "A2", "A3"],
};
```

C2 и A4 не добавляются автоматически. Это альтернативная модель для исследования; основной loads не меняется. Подготовка покажет остальные контакты на сетях как возможные пропуски.

## 4. PI: импеданс питания и переход к нагрузке процессора

```ts
const piBase = a.pi()
  .components(electricalComponents)
  .stackup(projectStackup)
  .materials(materials)
  .sources(sources)
  .temperatureC(25);

const ac = await piBase
  .loads(loads)
  .domain("frequency")
  .frequency({ startHz: 1e3, stopHz: 1e8, points: 201, scale: "log" })
  .run();
a.plot({ title: "Импеданс VCORE в точке SoC", data: ac.impedance(coreProbe) });

const switchingCoreLoad = {
  designator: "U1", rail: net("VCORE"), currentA: 2.0,
  waveform: { kind: "step", from: 2.0, to: 3.0, atS: 1e-6, riseS: 100e-9 },
};
const transient = await piBase
  .loads([switchingCoreLoad, ...otherLoads])
  .domain("time")
  .initial("steady")
  .time({ stopS: 20e-6, sampleStepS: 5e-9 })
  .run();
a.check({ name: "Минимум VCORE при скачке", actual: transient.voltage(coreProbe).min(), min: 1.0, unit: "V" });
a.plot({ title: "VCORE при включении нагрузки", data: transient.voltage(coreProbe) });
```

Здесь нагрузка специально задана без pin-overrides: все четыре питания изменяются с 0.5 до 0.75 A, возвраты тоже. Это иной распределённый сценарий, чем DC выше; builder его не наследует. Частотный PI — малосигнальный анализ рабочей точки, а не проигрывание step. Одни токи не задают выходной импеданс источников и паразитики конденсаторов: они находятся в ресурсах.

100 MHz — исследуемая граница примера, не обещание точности упрощённых ресурсов до этой частоты. Отчёт обязан показать диапазон применимости. Исследование внутренних токов корпуса и подробного ground bounce требует более полной модели.

## 5. Thermal: SoC, RAM и радиатор в горячем воздухе

```ts
const componentHeat = [
  { target: component("U1"), node: "junction", powerW: 2.2 },
  { target: component("U2"), node: "junction", powerW: 0.5 },
  { target: component("U3"), node: "junction", powerW: 0.3 },
  { target: component("U4"), node: "junction", powerW: 0.4 },
  { target: component("U5"), node: "junction", powerW: 0.6 },
];
const cooling = {
  boardSurfaces: profiles.cooling.sbcBoard,
  heatsinks: [{
    target: component("U1"), thermalNode: "case",
    model: models.thermal.sbc.heatsink,
    interface: models.thermal.sbc.interfacePad,
    cooling: models.thermal.sbc.heatsinkCooling,
  }],
};
const thermalBase = a.thermal()
  .components(thermalComponents)
  .stackup(projectStackup)
  .materials(materials)
  .heat(componentHeat)
  .cooling(cooling)
  .initialC(25)
  .time({ stopS: 600, sampleStepS: 1 });

const heat = await thermalBase
  .environment(profiles.environments.hot)
  .run();
a.plot({ title: "Температура всей платы через 600 s", data: heat.temperatureMap, atS: 600 });
a.plot({ title: "Температура кристалла SoC", data: heat.temperature({ target: component("U1"), node: "junction" }) });
a.check({
  name: "Температура SoC в заданном режиме",
  actual: heat.temperature({ target: component("U1"), node: "junction" }).max(),
  max: 85, unit: "degC",
});
```

Здесь мощность компонентов задана отдельно от токов питания; это самостоятельный тепловой сценарий. Потери dc.losses намеренно не добавлены, чтобы не учесть дважды компоненты, уже представленные componentHeat. Для совместного сценария нужно передать подходящий набор потерь и убрать соответствующие ручные источники. Карта lossMap — визуализация, а не физический набор источников для losses.

Ресурсы thermal содержат тепловые пути кристалл → корпус/плата, теплоёмкости и геометрические привязки; размеры корпуса одни не дают температуру кристалла. У радиатора есть инерция и характеристика теплоотдачи при заданных условиях. Воздух не решается как поток; профиль задаёт окружающую температуру. Предел 85 °C учебный, не паспортный предел Raspberry Pi/SoC.

Для платы, нагретой электрическими потерями, без отдельного componentHeat:

```ts
const fromElectricalLosses = await a.thermal()
  .components(thermalComponents)
  .stackup(projectStackup)
  .materials(materials)
  .losses(dc.losses)
  .cooling(cooling)
  .environment(profiles.environments.hot)
  .initialC(25)
  .time({ stopS: 600, sampleStepS: 1 })
  .run();
```

Этот запуск использует только потери, которые DC действительно умеет вычислить и пространственно привязать. Агрегированное потребление IC не доказывает, что вся входная мощность рассеивается в junction. Неизвестные компонентные теплопотери должны быть явно отмечены; такой пример не выдаётся за полную тепловую модель компьютера.

## 6. Электротепловая обратная связь

```ts
const coupled = await a.electrothermal()
  .stackup(projectStackup)
  .materials(materials)
  .electrical({ components: electricalComponents, sources, loads })
  .thermal({
    components: thermalComponents,
    heat: componentHeat,
    cooling,
    environment: profiles.environments.hot,
    initialC: 25,
  })
  .couplings(models.thermal.sbc.copperAndPassiveCouplings)
  .time({ stopS: 600, sampleStepS: 1 })
  .run();
a.plot({ title: "Нагрев с изменением электрических потерь", data: coupled.thermal.temperatureMap, atS: 600 });
```

Ресурс copperAndPassiveCouplings содержит проверенные привязки только меди и поддерживаемых пассивных компонентов: их потери поступают в thermal, температуры — обратно в электрические модели. Он не включает уже заданный вручную нагрев U1–U5. Такой состав исключает двойной учёт и не обещает автоматически узнать температурную зависимость потребления SoC. Общая time одна; два готовых независимых результата DC/thermal сами по себе не являются двусторонним расчётом.

Точная сериализация пространственных couplings остаётся открытым контрактом ресурса. Это обязательный предоставленный вход примера, не автоматически существующая встроенная модель.

## 7. Дифференциальная линия: геометрия, импеданс, TDR, SI и eye

```ts
const usbTxP = a.port({ positive: pad("U3", "3"), negative: pad("U3", "2") });
const usbTxN = a.port({ positive: pad("U3", "4"), negative: pad("U3", "2") });
const usbRxP = a.port({ positive: pad("J2", "3"), negative: pad("J2", "4") });
const usbRxN = a.port({ positive: pad("J2", "2"), negative: pad("J2", "4") });
const usbP = a.channel({ from: usbTxP, to: usbRxP });
const usbN = a.channel({ from: usbTxN, to: usbRxN });
const usbPair = a.differential({ positive: usbP, negative: usbN });

const geometry = await a.geometry()
  .scope([net("USB_DP"), net("USB_DM"), net("GND")])
  .rules([
    { kind: "connectivity" },
    { kind: "clearance", minMm: 0.15 },
    { kind: "skew", channels: [usbP, usbN], maxMm: 0.5 },
    { kind: "reference-continuity", channel: usbP },
    { kind: "reference-continuity", channel: usbN },
  ])
  .run();
a.check({ name: "Геометрия USB-пары", actual: geometry.violations.count(), equals: 0 });

const impedance = await a.impedance()
  .target(usbPair)
  .stackup(projectStackup)
  .materials(materials)
  .method("layered")
  .frequency({ startHz: 1e6, stopHz: 2e9, points: 201, scale: "log" })
  .run();
a.plot({ title: "Дифференциальный импеданс участков пары", data: impedance.characteristicImpedance });

const tdr = await a.tdr()
  .target(usbPair)
  .stackup(projectStackup)
  .materials(materials)
  .referenceOhm(90)
  .termination("matched")
  .riseS(500e-12)
  .time({ stopS: 10e-9, sampleStepS: 20e-12 })
  .run();
a.plot({ title: "TDR дифференциальной пары", data: tdr.impedance });

const signal = await a.si()
  .channels([usbPair])
  .stackup(projectStackup)
  .materials(materials)
  .drivers([{
    ports: { positive: usbTxP, negative: usbTxN },
    model: models.electrical.sbc.differentialDriver,
    stimulus: { kind: "prbs", order: 7, seed: 1, bits: 1024,
      bitRateBps: 480e6, riseS: 500e-12, fallS: 500e-12 },
  }])
  .receivers([{
    ports: { positive: usbRxP, negative: usbRxN },
    model: models.electrical.sbc.differentialReceiver,
  }])
  .time({ stopS: 1024 / 480e6, sampleStepS: 20e-12 })
  .run();
const eye = await a.eye()
  .signal(signal.differentialVoltage({ positive: usbRxP, negative: usbRxN }))
  .bitRateBps(480e6)
  .discardS(20e-9)
  .phaseS(0)
  .thresholdV(0)
  .run();
a.plot({ title: "Глаз дифференциального сигнала", data: eye.diagram });
```

Проверки и числа иллюстративны; generic PRBS/driver не симулирует протокол USB и не заменяет USB compliance. termination matched в TDR — явно выбранная измерительная модель; SI использует отдельный ресурс реального/приближённого приёмника на дальнем конце. Кабель и удалённое устройство геометрически не моделируются, их нагрузка представлена ресурсом receiver.

Для differential target referenceOhm относится к дифференциальному режиму; ресурс должен также задавать необходимое поведение общего режима. characteristicImpedance описывает поддерживаемые участки линии, а не одно универсальное Z0 для всей трассы с vias. TDR показывает неоднородности. differentialVoltage вычитает напряжения двух явно выбранных портов; его форма добавлена в справочник как accessor будущего результата.

drivers/receivers и channels приняты. Карта ports использует [согласованные привязки](dsl-working-contract.md): ресурсы объявляют порты positive/negative с совместимыми references; это совместная модель общего/дифференциального режима, не два независимо рассчитанных сигнала. Другие активные источники в этом примере не заданы, но существенная медь/нагрузки остаются в извлечённой модели. Для двунаправленной линии это один выбранный режим передачи, а не автоматический перебор всех направлений.

## 8. EMI-риски и подробная проверка выбранной пары

```ts
const emi = await a.emi()
  .kind("risk")
  .signals([{ channel: usbP, riseS: 500e-12 }, { channel: usbN, riseS: 500e-12 }])
  .rules(profiles.checks.emi)
  .run();

const detailed = await a.em()
  .target(usbPair)
  .stackup(projectStackup)
  .materials(materials)
  .method("full-wave")
  .frequency({ startHz: 1e7, stopHz: 2e9, points: 201, scale: "log" })
  .space({ kind: "free-space" })
  .outputs(["sParameters", "fields"])
  .run();
```

Риски EMI — правила с объяснениями, не численное заключение об излучении всего компьютера. Full-wave — отдельный явно запрошенный расчёт выбранной структуры вместе с существенными возвратными путями и окружением. Backend и совместимые модели должны быть предоставлены; их отсутствие не вызывает незаметный переход к простому Z0. Поля при получении S-параметров соответствуют нормированным возбуждениям портов, не неизвестной активности компьютера. Для излучения рабочего устройства нужны конкретные возбуждения; кабели/корпус пока исключены.

## 9. Сравнение холодной и горячей среды без изменения платы

```ts
const environments = await a.sweep({
  parameter: {
    name: "environment",
    values: [profiles.environments.cold, profiles.environments.room, profiles.environments.hot],
  },
  evaluate: async (environment) => {
    const result = await thermalBase.environment(environment).run();
    return result.temperature({ target: component("U1"), node: "junction" }).max();
  },
});
a.plot({ title: "Температура SoC в разных условиях", data: environments.series });
const report = a.report();
```

Во всех вариантах начальная температура остаётся 25 °C, как задано в thermalBase. Если нужны холодные/горячие старты, callback явно добавляет initialC("ambient"). Функции подбора solve/optimize устроены аналогично, но получают цель/ограничения. Повторное использование переменных не меняет соседние конфигурации.

## 10. Пример на реальном экспорте из репозитория

Проверен [board.json STM32/CAN/RS-485](../../../../copilot-router/tests/pcb-routing/stm32g0_can_rs485_easyeda/board.json). Из файла прочитаны именно соответствия компонент/вывод/сеть, а не выведены из имени микросхемы:

| Объект | Проверенная связность |
| --- | --- |
| J1 | 1 — VIN_RAW, 2 — GND |
| U2 | 5 — +3V3, 2 — GND; 1 и 3 — +5V |
| U3 | 4/5/6 — +3V3, 7 — GND |
| U4 | 3 — +5V, 5 — +3V3, 2/8 — GND; 7 — CAN_H, 6 — CAN_L |
| U5 | 8 — +3V3, 5 — GND; 6 — RS485_A, 7 — RS485_B |
| J4 | 1 — CAN_H, 2 — CAN_L, 3 — GND |
| J5 | 1 — RS485_A, 2 — RS485_B, 3 — GND |

Это подтверждённая связность сохранённого экспорта, **не аудит корректности электрической схемы или паспортных ролей выводов**. В частности, три контакта U3 на +3V3 считаются точками нагрузки только в рамках выбранного equal-приближения. Fixture может содержать незавершённую разводку; см. его [README](../../../../copilot-router/tests/pcb-routing/stm32g0_can_rs485_easyeda/README.md). Перед физическим анализом нужен окончательный snapshot с реальной заливкой и проверкой связности.

Следующий блок — **отдельный сценарий**, где host передаёт именно эту плату как board, её фактический двухслойный стек и модели. Он не запускается на SBC из разделов выше.

```ts
const a = analysis({ board });
const railSource = a.port({ positive: pad("U2", "5"), negative: pad("U2", "2") });
const mcuProbe = a.port({ positive: pad("U3", "4"), negative: pad("U3", "7") });
const controllerLoads = [
  { designator: "U3", rail: net("+3V3"), currentA: 0.030,
    pinCurrentsA: { "4": 0.020 }, returnPinCurrentsA: { "7": 0.030 } },
  { designator: "U4", rail: net("+3V3"), currentA: 0.005 },
  { designator: "U5", rail: net("+3V3"), currentA: 0.020 },
];
const dc = await a.dc()
  .components(models.electrical.controller.componentDescriptions)
  .stackup(profiles.stackups.controller)
  .materials(models.materials.controller)
  .sources([{ port: railSource, voltageV: 3.3, model: models.electrical.controller.railSupply }])
  .loads(controllerLoads)
  .temperatureC(25)
  .run();
a.check({ name: "Питание MCU", actual: dc.voltage(mcuProbe), min: 3.1, unit: "V" });
a.plot({ title: "Потери разводки 3.3 V", data: dc.lossMap });

const canP = a.channel({
  from: a.port({ positive: pad("U4", "7"), negative: pad("U4", "2") }),
  to: a.port({ positive: pad("J4", "1"), negative: pad("J4", "3") }),
});
const canN = a.channel({
  from: a.port({ positive: pad("U4", "6"), negative: pad("U4", "2") }),
  to: a.port({ positive: pad("J4", "2"), negative: pad("J4", "3") }),
});
const canPair = a.differential({ positive: canP, negative: canN });
const geometry = await a.geometry()
  .scope([net("CAN_H"), net("CAN_L"), net("GND")])
  .rules([{ kind: "connectivity" }, { kind: "clearance", minMm: 0.2 }])
  .run();
a.check({ name: "Связность CAN на плате", actual: geometry.violations.count(), equals: 0 });
```

Для U3 получаются 20/5/5 mA на контактах 4/5/6 и 30 mA возврата через 7. У U4 нагрузка 3.3 V возвращает по 2.5 mA через 2/8. Токи/пороги — учебные сценарные значения, не измеренные параметры этой платы.

componentDescriptions — предоставленный массив физических моделей, включая явную границу выхода U2 и поведение многопитаемого U4 для данной задачи. Здесь анализируется только разводка выбранной шины: без такого ресурса нельзя считать, что питание +5V и внутренние связи регулятора/трансивера исчезли сами. Для полного рабочего режима нужно добавить источники/нагрузки других шин и соответствующие модели. В текущем fixture стек и приборные ресурсы не подтверждены; prepare должен показать эти пробелы. Этот пример проверен по номерам/сетям и синтаксису, но физический решатель не запускался.

## 11. Что обязательно предоставить для повторения на своей плате

- Фактический board с медью, Pour/Fill, отверстиями и связностью; не только схема или картинка платы.
- Стек и свойства материалов, соответствующие изготовлению.
- Источники с их выходным импедансом, потребление по шинам и режимы нагрузки.
- Компонентные модели, достаточные выбранным анализам; отдельные ресурсы тепловых путей, корпуса/радиатора и SI.
- Осмысленные пределы check из требований проекта. Числа учебного SBC не являются готовыми нормативами.

Результат должен отличать рассчитанное по входам от предположенного. Наличие наглядного примера не означает, что нужные ресурсы уже существуют в каталоге или что выбранная модель точна для любой платы.
