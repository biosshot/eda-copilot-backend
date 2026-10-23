# Задание: геометрическая валидация PCB через Analysis DSL

Независимое направление параллельно P7b и Thermal. База d8ca0dc: существующие RoutingBoardV2, импорт, физическая медь/связность, process runtime и DSL. Пользователь поручил реализацию, тестирование и ревью через субагента. Работа в отдельном worktree; итоговый commit/merge делает родитель после проверки.

## Результат и граница

Реализовать production `a.geometry()` для `connectivity`, `clearance`, `edge-clearance` с данными нарушений, измерениями и понятными отчётами по реальной плате. Это не новый geometry importer и не переписывание принятых polygon/connectivity алгоритмов.

Поддержать реальные проводящие regions, включая Pour/Fill и отверстия, и scoped selectors по действующему DSL. Выдать source IDs/net/layer, измеренную величину, требование, место нарушения и исходную геометрию для overlay. Не смешивать электрические нарушения с численной неопределённостью.

Не включать: extraction каналов/vias, длины/skew/return-transition/reference-continuity без готовой корректной топологии, SI/EMI/full-wave, route/refill/автоматическое исправление CAD, новые V2 поля, перестройку DC/FEM, GPU/F32, универсальный DRC/DFM. Unsupported rule не должен молча считаться пройденным. Не исправлять известные редкие ограничения геометрии ценой новых глобальных тяжёлых представлений.

## Семантика до реализации

Прочитать AGENTS/CHANGELOG, geometry semantics/verification, принятые ограничения, DSL types/schema/prepare и native geometry report. Зафиксировать `geometry-validation-semantics.md`: область каждого правила, определения расстояния, scope, идентичности и агрегирования результатов. Сообщить родителю о пробелах; продолжить код в согласованной области.

- **Clearance:** кратчайшее геометрическое расстояние между физическими замкнутыми областями проводников на одном медном слое, с корректным учётом holes. Проверять разные известные nets; не считать совпадение пустых/неизвестных имён доказательством одной сети. Для unknown net описать явную conservative policy/diagnostics. Геометрическое перекрытие даёт нарушение и 0-distance, а не отрицательный выдуманный зазор. Не проверять projected clearance разных слоёв как нарушение одного слоя. Учитывать tolerance и пограничные случаи, не увеличивать допустимый clearance молча.
- **Scope:** выбранные объекты проверяются против физически релевантной окружающей меди, а не только друг против друга, если DSL не задаёт иного. Нужны стабильные дедупликация и IDs. Правило одной и той же сети явно описать; оно не означает, что несвязанные области стали электрически соединены.
- **Edge clearance:** расстояние меди до реального внешнего board outline и физических вырезов; отличать края платы от обычных plated/NPTH drill holes, если rule не включает drill clearance. Медь вне контура/в вырезе — отдельное подтверждённое нарушение, даже при положительном расстоянии до ближайшего сегмента. Отсутствующий/некорректный outline не даёт pass. Не использовать bounding box вместо формы платы.
- **Connectivity:** переиспользовать reliable/uncertain evidence действующего графа. Подтверждённые shorts показывать с участвующими nets/source IDs. Несколько islands одной сети — физический факт, который нельзя без заявленного требования автоматически назвать конкретным обрывом; определения open/unconnected pads связать с доступным netlist intent и поддержанным правилом. Если информации для open-check недостаточно, явно показывать непроверенное требование. Не объединять проводники по имени сети. Не вводить второй конфликтующий connectivity solver.
- **Incomplete input:** stale/unsupported fill, uncertain contact/drill topology и неподдержанная геометрия сохраняются в diagnostics/coverage. Ноль обнаруженных нарушений при неполном покрытии не является доказанным pass: `violations.count()`/checks должны сохранять conditional/unavailable смысл. Для подтверждённых найденных нарушений сохранять доказательность отдельно от отсутствующих проверок. Не обнулять результат при ошибке вычисления.

Результаты должны быть детерминированными относительно порядка input objects. Указать единицы mm, размерности, equality at threshold и правила численной погрешности. Проверка ниже/выше порога на величину намного больше tolerance должна быть однозначной.

## Реализация и пример

Собственный Rust CPU/F64 модуль проверки поверх существующего geometry preparation; отдельный TS executor, общий addon/process/runtime. Использовать пространственный индекс для кандидатов, точную поддержанную polygon distance/intersection для решения. Не делать все попарные проверки всей большой платы без broad phase и budget. Не переносить физическую геометрию и solver в TS.

```js
const a = analysis();
const g = await a.geometry().rules([
  { kind: 'connectivity' },
  { kind: 'clearance', minMm: 0.2 },
  { kind: 'edge-clearance', minMm: 0.3 },
]).run();
a.plot({ title: 'Geometry measurements', data: g.measurements });
a.check({ name: 'No confirmed violations', actual: g.violations.count(), equals: 0 });
```

Использовать действующие `scope`, `Rule`, `GeometryResult` и DataValue. Дополнять поля результата только при необходимой измерительной информации; не менять пользовательские factories ради адаптера. В `.prepare()` отделить structural validity от физической готовности. `check` не прекращает исполнение. Написать прямые `.js` DSL fixtures, тонкие `.ts` launchers; helper не генерирует сам сценарий.

## Тесты и реальные платы

Независимые аналитические coupons: пара прямоугольников с известным зазором, касание/перекрытие, point-to-segment minimum, диагональные участки, polygon holes, медь в вырезе/за пределами outline, concave board, разные слои, разные/одинаковые/unknown nets, удалённые islands, barrels/contacts, scope against neighbour outside scope, duplicate identity/order. Ожидания не вычислять тем же production helper.

Негативные inputs: отсутствующий outline, partial fill, известная uncertain connectivity, invalid limits, unsupported rules/selectors, timeout/cancel/memory/report budgets и следующий успешный job. Сохранить ранее принятые геометрические/DC регрессии. Не снижать честность coverage ради зелёных checks.

Использовать **все восемь** имеющихся board fixtures: отдельный named `validation.js` + launcher и versioned expectations. Каждый отчёт содержит три вида правил там, где достоверно доступны данные, а отказ/unknown при отсутствии требуемого input проверяется явно. Нельзя менять исходную плату, clearance или выбор объектов только ради pass; допустимы реальные нарушения. Для аналитических controlled pass/fail использовать отдельные coupons. Сохранить named check с ожидаемым fail и последующее выполненное действие.

HTML-отчёт: обзор платы, слои/медь/outline/cutouts, фильтры rule/layer/status, подсветка объектов каждой находки, кратчайший отрезок/место конфликта, таблица actual/required/source IDs, coverage/limitations. Результат должен быть пригоден для ручного контроля, а не только счётчик. Для каждой платы сохранить exact DSL, board/source/native hashes, numerical JSON, audit, HTML и screenshots; открыть отчёты и проверить их читаемость.

Версионный audit сверяет identity, units, нет потерь/дубликатов findings, геометрию witness, правил/порогов, coverage и count. Negative tests: убрать finding, подменить ID/layer/distance/threshold, потерять diagnostic, promote uncertain to pass, пустые/укороченные/дублированные массивы. Сохранённые полигоны и distance witnesses проверять независимо от renderer. Простое совпадение count с самим собой недостаточно.

Измерить время/RSS и количество broad-phase candidates/exact checks; задать bounded limits до выделений. Нет обязательного caching или общего runtime. Выдать понятную ошибку при достижении лимита, не false-complete с неполным списком.

Исполнитель запускает focused Rust/Node, релевантные geometry/DC/runtime regression, native build/typecheck/build/installed-package path и восемь named fixtures. Тяжёлые native/package команды координировать с родителем. Сохранить exit codes/hashes/actual evidence в `geometry-validation-verification.md`, обновить docs/types/README/CHANGELOG минимально.

Приёмка: все три правила работают в документированной области, неполные данные не дают ложного pass, сохранены existing geometry/DC semantics, есть независимые/negative tests и восемь проверенных визуальных отчётов. Неподдержанные length/skew/reference rules остаются явным unsupported. Передать родителю на ревью, исправить замечания; самостоятельно не коммитить и не начинать другой анализатор.
