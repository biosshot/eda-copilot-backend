# P7-B1: устранить блокер геометрии полного STM32 reference

2026-10-31. Реализация разрешена. Это первая часть B [единого плана](p7-real-channel-closure-plan.md), prerequisite полноценной EM-проверки. Остальные B (политика обрезки больших проводников) и C/D не считаются выполненными этим заданием.

## Цель

На исходном полном STM32 и исходном whole-source request получить корректную геометрию, прошедшую area/topology/contact проверки, без удаления меди/отверстий/ветвей и без ослабления физических guards. Затем проверить, что исправленная геометрия не ломает существующий малый AC extraction. Не требуется закончить полный STM32 EM solve или весь P7.

Прочитать AGENTS.md, CHANGELOG, родительский план, p7-route-partition-results.md (frozen ledger), p7-oriented-boundaries-results.md, p7-memory-architecture-audit.md. Особое внимание: прежняя maximal-safe grid попытка уже провалилась на другом annulus; повторять её как новое решение нельзя.

## Исходное воспроизведение

- native/analyze/tests/stm32_joint_precision.rs сейчас ожидает отказ; заменить позитивной проверкой только после настоящего исправления, негативные precision/topology controls сохранить.
- tests/fixtures/analysis-local-extraction/board-integration/stm32-joint-reference-request.json и tests/pcb-analyze/STM32G0_CAN_RS485/STM32G0_CAN_RS485.json — неизменные исходные входы.
- Исторический отказ: относительное изменение площади3.974439737e-8 при guard1e-8; после одного прежнего safe-grid trial другой источник1.418278274e-8. Полная область60.1×40.1mm; просто поднять глобальный dyadic scale упирается в безопасный integer range. Компоненты/holes в том случае совпадали, но это само по себе не разрешение переписать reported area.

## Файлы и полномочия

Владение: native/analyze/src/geometry/context.rs, непосредственно нужные geometry helpers, native/analyze/src/extraction/local/board_mesh.rs, native/analyze/tests/stm32_joint_precision.rs, targeted geometry/contact tests и новые tests/fixtures/analysis-p7-geometry/, docs/experimental/analyze/p7-b-geometry-results.md. Разрешены необходимые изменения внутреннего координатного/overlay пути. Это не audit-only задача: довести выбранное решение до кода и позитивных тестов.

Не менять TS planner/DSL/сборщик, магнитный/электрический алгоритм, F64, dependencies, лимиты/guards, исходные платы/whole-source request, чужие tests/harness и общие CHANGELOG/roadmap. Changelog-ready описание дать в собственном отчёте для родителя. Не трогать native/pcb-board-packer и пользовательский native/analyze/debugging. Не коммитить/пушить, не запускать субагентов.

Сосед /root/p7_a_em_baseline владеет сценариями/runner/reference manifests. Ты владеешь cargo/native build; согласуй build window и затем передай ему готовый addon hash. Не заменять используемый им бинарник во время запуска. Один тяжёлый run одновременно.

## Порядок

1. Локализовать потерю точности по операциям: original rings→common coordinates→boolean/intersection→arrangement→barrel/contact→mesh. Сравнить физические контуры/площади/топологию, а не только итоговый exception. Описать выбранное исправление до реализации в своём отчёте.
2. Выбрать одно простое корректное решение в существующей геометрической библиотеке: достаточный безопасный диапазон/локализация/согласованный overlay, с сохранением общей конформной границы. Не менять только audit denominator/полигон reported area; не round/snap через реальные gaps. Локализация должна сохранять совпадение соседних доменов/контактов. Не начинать отдельное семейство geometry engines или несколько fallback branches.
3. Позитивный whole STM32 build исходного запроса: complete source/barrel/contact mapping, conserved areas/volumes в прежних допусках, topology/holes, конечные контакты. Проверка просто «ошибка стала другой» недостаточна. Если следующий независимый blocker появился — исправить в этих границах либо явно сообщить incompleteness, не выдать pass.
4. Регрессии: translated/rotated equivalent малые физические coupons, широкий контур с тонким annulus, близкие реальные gap/bridge, several barrels/spans/oriented contacts, разделение одной меди на source objects. Истинные gaps не замыкать. Принятый ранее редкий curve-approximation limitation не расширять в отдельную кампанию и не скрывать.
5. После geometry tests — обычная npm run native:build и малый существующий AC/EM reference на неизменной физике (section/local/via, где прежний результат положителен). Проверить комплексный отклик, не только DC area/current. Не выдавать старый неподтверждённый via convergence case за успешный baseline. Согласовать numerical case с A, reuse существующие эталоны.
6. Geometry cost на exact real request: time, counts, OS RSS/private commit, bounds и phases; проверить, не заменили ли ошибку огромным memory regression. Полный магнитный whole-board run для приёмки B1 не нужен; не тратить на него часы.

## Приёмка и ограничения

Положительная неизменная STM32 geometry + целевые tests в старых guards + одинаковая физика малого AC/EM контроля + штатная native build. Код reviewable, без экспериментальных switches/веток. Отчёт: причина, путь исправления, точные source lines, команды, before/after, CPU/binary/inputs hashes, ресурсные числа и оставшиеся ограничения. Геометрический успех не является8%/15% физической точностью канала.

Использовать bounded supervisor/timeout, до8GiB и600s на диагностическую попытку при достаточном host headroom; существенно меньше для локальных tests. Не повышать бюджеты автоматически. Не повторять неизменные failures. Если корректный фикс требует изменения сторонней зависимости/guard — описать конкретную необходимость родителю; не делать молча. Полезная реализация в разрешённых geometry файлах уже авторизована, отсутствие старого helper не повод остановиться на resistor prototype.

После выполнения ждать ревью родителя, не переходить самовольно к C/D или менять поведение широких опор без EM-доказательства.
