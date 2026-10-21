# Разделение ответственности DSL: принят вариант B

Дата: **2026-10-20**. Согласованная рабочая основа, без реализации. Полный текущий API и примеры: [справочник](dsl-reference.md).

## Принятое решение

| Уровень | Ответственность |
| --- | --- |
| analysis({ board }) / analysis() | Snapshot платы либо работа без платы; общий отчёт |
| ic/resistor/capacitor/inductor/device | Физические описания с designator, роли/привязки, свойства моделей; без сценарного потребления |
| stackup(...) | Переиспользуемое описание физических слоёв; без передачи Routing DSL контекста |
| a.dc(), a.thermal() и другие builders | Отдельные физические дополнения и условия конкретного расчёта |
| builder.prepare() | Проверка входов и отчёт происхождения/пригодности, без решения |
| builder.run() | Автоматическая подготовка и численный запуск |
| a.check/plot/log/report | Проверки, визуализация и накопление результатов |
| a.sweep/solve/optimize | Обычные объектные вызовы с callback evaluate |

Компоненты передаются массивом, например ic({ designator: "U1", ... }). Не требуется перечислять весь BOM. Одна переменная может использоваться в нескольких builders. Их конфигурации изолированы; методы возвращают новые значения. Запуск происходит только через run(), без thenable-магии. Крупные разделы принимают обычные JS-объекты, например time({ stopS, sampleStepS }).

## Пример нескольких контекстов в одном сценарии

```ts
const a = analysis({ board });
const components = [ic({ designator: "U1", model: models.electrical.u1 })];
const thermalComponents = [ic({ designator: "U1", model: models.thermal.u1 })];

const dc = await a.dc()
  .components(components)
  .stackup(profiles.stackups.project)
  .materials(models.materials.project)
  .sources(sources)
  .loads(loads)
  .temperatureC(25)
  .run();

const heat = await a.thermal()
  .components(thermalComponents)
  .stackup(profiles.stackups.project)
  .materials(models.materials.project)
  .losses(dc.losses)
  .environment(profiles.environments.hot)
  .cooling(profiles.cooling.project)
  .initialC(25)
  .time({ stopS: 600, sampleStepS: 1 })
  .run();
```

sources/loads и ресурсы предоставлены сценарием. Losses передаются явно; мощность уже учтённого объекта не добавляется повторно через heat. Это односторонняя передача потерь. Для обратной связи температуры и потерь используется electrothermal с electrical/thermal/couplings и общим временем.

Среда не живёт в analysis или компоненте. Холодный/горячий thermal и электрический расчёт при заданной температуре могут находиться в одном отчёте без скрытого наследования. Power — обычный объект sources/loads для переиспользования; отдельного обязательного a.power нет.

## Что было рассмотрено и не выбрано

- A: полностью независимые функции dc({ board, ... }); требует повторной передачи платы и отдельной организации отчёта.
- B: небольшой контекст платы/отчёта и самостоятельные конфигурации анализов — выбран пользователем, дополнен builders.
- C: обязательный граф task/dependencies/runAll — не выбран, добавляет отдельные понятия для простых сценариев.

Старые варианты синтаксиса удалены из действующих примеров. Это не отказ от возможного внутреннего планировщика: его архитектура не задаётся пользовательским DSL.

## Автоматика и открытые вопросы

Ручное поле полностью заменяет автоматически полученное; ручные списки не дополняются. Отчёт содержит фактические значения и источники, предупреждения и область пригодности результатов. Неразрешимая обязательная неоднозначность блокирует соответствующий расчёт, а не превращается в ноль. check не останавливает программу.

Автодетекция ground/supplies как обязательная операция для каждой детали **не утверждена**. Принята более простая [нагрузка компонента на сети с равномерным распределением и отдельными токами возврата](dsl-components-simple.md). Это не меняет принятого разделения DSL.

Полные типы ресурсов, конкретные numerical methods, правила выбора контактов, sandbox исполнения и итоговая перепроверка плана остаются открытыми. Исследовавшиеся внешние подходы — [PyBaMM: model/parameter values/experiment/simulation](https://docs.pybamm.org/en/stable/source/api/simulation.html) и [OpenMDAO: components/connections/problem](https://openmdao.org/newdocs/versions/latest/basic_user_guide/basic_modeling/component_types.html); это примеры разделения ответственности, не зависимости проекта.
