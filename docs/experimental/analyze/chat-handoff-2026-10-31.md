# Передача контекста анализаторов — 2026-10-31

## С чего продолжить

Пользователь перенёс работу в другой чат. Последнее поручение здесь — сохранить контекст и обновить документацию, без ревью завершившегося D1, новых расчётов и запуска агентов. D1-агент завершился и сообщил NO-GO. Его изменения и выводы **ещё не прошли родительское ревью**. Следующий полезный шаг — прочитать его отчёт, проверить код/доказательства и обсудить решение с пользователем; не считать D1 принятым и не запускать очередной перебор методов.

Backend: `D:/MyProject/NN/projects/Agents/easyeda-copilot/eda-copilot-backend`. Общая рабочая папка на один уровень выше. Shell PowerShell. Читать backend AGENTS.md/CHANGELOG перед изменениями. В этом handoff сохранены принятые результаты предыдущих ревью и отдельно сведения последнего агента; новые solver tests здесь не запускались.

## Цель проекта

Собственный переносимый набор PCB-анализаторов в eda-copilot-backend: Rust-ядро `native/analyze`, TypeScript/JavaScript DSL и исполнение `src/analyze`. PCB packer и analyzer — Rust-библиотеки, общая Node N-API оболочка в `native/node-addon`; один `.node`. RoutingBoardV2 — транспорт от CAD, сразу переводится во внутреннюю модель. Самодостаточный problem → worker → Rust → результат, с отменой/лимитами/кэшем. Не создавать заранее общий compute-runtime и новый репозиторий типов.

Математика анализатора CPU/F64, многопоточность по реальной пользе и памяти. GPU на будущее. Placement/packer имеет отдельные F32-правила; они не требуют перевести analyzer на F32. Прошлые magnetic F32/уменьшения порядков не прошли проверки и не являются выбранным рабочим путём. Обязательных внешних solver-программ нет: промышленный софт/научные работы/openEMS — источники методов и эталонов, не новый `.solver('openems')`.

Согласованный будущий состав: geometry/DC, thermal/transient cooling, electrothermal, AC/transient PI, impedance/TDR, SI/crosstalk/eye/timing, networks/S-параметры, EMI/внешние возбуждения, планарный full-wave/антенны, sweep/solve/optimize и расчёты без платы. Это целевой состав, не список уже реализованного. BER, механика, CFD, корпус/кабели и универсальная SPICE-симуляция отложены. Радиатор/вентилятор учитываются параметрической характеристикой охлаждения. CAD 3D-модели не импортируем; корпус/footprint помогает приближённой тепловой геометрии.

## Основные документы

- [Общий план P0–P15](implementation-plan.md): состав/зависимости. Нижние датированные статусы исторические, не текущие сертификаты.
- [Единый план закрытия P7](p7-real-channel-closure-plan.md): главный текущий архитектурный план, A–H. Прежние `next-task-*` не являются одновременно активными.
- [Архитектура модулей](implementation-structure.md), [математические методы](numerical-architecture.md).
- [DSL reference](dsl-reference.md), [рабочий контракт](dsl-working-contract.md), [решения сценариев](dsl-scenario-decisions.md), [примеры плат](dsl-board-scenarios.md).
- [RoutingBoardV2 TypeScript](routing-board-v2-shape.md), [аудит достаточности](routing-board-v2-sufficiency.md), [корпус/валидация](validation-corpus.md).
- [Аудит магнитной памяти](p7-memory-architecture-audit.md), [HF prerequisite](p7-hf-operator-design.md), [P6/P7 интеграция и кэш](p6-p7-integration-acceptance.md).
- [Текущее задание D1](next-task-p7-d1-coupled-assembly.md) и **[его новый отчёт, ожидающий ревью](p7-d1-coupled-assembly-results.md)**.

DSL согласован ранее, заново его не проектировать. JS-код выполняется последовательно, builders запускают расчёты через `.run()`, результаты используются в `a.check/a.plot` и следующих вычислениях. `a.check` записывает провал и не прерывает сценарий. Контексты физических параметров принадлежат конкретному анализу; используются globals board/profiles/models, компоненты — массив с designator. При отсутствии ручных токов по пинам ток выбранной сети/возврат распределяются поровну, допущения в отчёте; ручные значения приоритетны. SI: `.channels()`, drivers/receivers, `.timing()` результата; `a.differential()` — описание пары. EMI использует `.excitations()`. `a.model()` принимает универсальные модели, включая network parameters; Attenuator Pad — конкретная модель. Обычный PCB DSL не должен требовать пользовательского прямоугольного scope.

## Где остановился общий прогресс

Ранее созданы V2 exporter/fixtures, Node addon/worker, executable DSL, геометрия/связность/mesh, PCB DC, P6 электрические сети и часть extraction. Существуют реальные тесты плат/графики. Импорт и прохождение DC не означают EM-точность. Корпус начинался с восьми плат; исходная цель ≥20/полный аудит не объявляется выполненной этим чатом. P4 fallback refill и многие поздние возможности остаются открытыми.

Thermal развивался независимо; пользователь попросил остановить дальнейшую оптимизацию при базовой приемлемой работе. Принята политика допуска `max(0.1 °C, 1% от нагрева)`: [thermal policy](thermal-convergence-policy.md), [сокращение уточнений](thermal-refinement-stopping-verification.md), [extended transient](thermal-extended-transient-verification.md). Не возобновлять Thermal автоматически и не объявлять весь P9 завершённым по этой краткой сводке.

Основная текущая блокировка — P7. P8 (PI) и P11 (SI) зависят от соответствующей модели, полосы и проверенного P6 представления. Нельзя объявить их готовность по одному MHz-сечению или low-band graph.

## Почему застряли на P7

Нужно извлекать выбранные сигналы/группы из исходной меди вместе с физическими возвратами и соседями. Однородные участки считать совместным 2D polygon-section, неоднородности — существующим joint-volume R/C/L, затем сокращать и соединять через P6. Сечения уже учитывают внешнюю L через вакуумную C0. Joint-volume уже включает сохранённые проводники/поля; добавление ещё одной глобальной «земли» удвоит часть энергии.

Проблема не только в GND: любой возврат, VCC/AGND/DGND, физические щели/обходы/переходы и несколько сигналов. Имена сетей не соединяют медь. P6 связывает данные модели, но не извлекает отсутствующие межблочные магнитные взаимодействия.

Исторический большой magnetic: 1,411,442 unknowns, 784,635,174 factor values, **6,277,081,392 B только F64-значений фактора**. Это разреженная факторизация с fill-in, не просто случайно выделенный плотный массив; пик ещё выше из-за индексов/workspace/mesh/basis. Прошлые compact/F32 варианты не прошли физические/численные проверки. Пользователь приоритизирует снижение памяти и скорости; алгоритм не менять бесконтрольно, собственного out-of-core не делать, на OS swap не полагаться как на экономию private commit.

Выбранный план A–H: эталоны/корпус → геометрия/выбор области → совместные быстрые сечения → доказанная EM-сборка → память оставшихся joint блоков → полоса/потери/временное представление → реальные DSL 2/4/6layer/RF → очистка. Тестирование/ресурсные измерения идут с начала. Объёмный путь сохраняется для оправданных неоднородностей; не сваливать всю плату в него при первом отказе.

Ориентиры плана: normal accuracy8%, approximate15% только с проверкой; mesh/domain/cut sensitivity2% — отдельный критерий, не физическая ошибка. Предыдущие 5%/120s — исторические цели, не новая безусловная приёмка. Ресурсные цели обычного полного STM32300s/2GiB (120s stretch), сложного300s/4GiB, диагностика до600s/8GiB только по заранее обоснованному плану. Точные бюджеты отдельных заданий могут быть меньше. Low-band P8 handoff предполагает DC..1kHz с проверкой, нынешние местные1–100kHz модели нельзя просто экстраполировать. MHz/GHz, skin/dielectric losses, causal realization — самостоятельные незакрытые части F.

## Принято родительским ревью в этом чате

### A — проверочная база

[Independent accuracy](p7-a-em-accuracy-results.md), [DSL validation](p7-a-dsl-validation-results.md). Малый finite35um coupled stripline сравнен с независимым Cartesian Laplace reference. C/L/Z/complex S: max0.07053065% плюс заранее назначенная2% engineering uncertainty, а не строгий лабораторный bound. Настоящий DSL6.125s, sampled tree private968642560B; 131 numerical value точно совпал с историческим native regression. TEM velocity identity сама по себе не проверяет абсолютную C/L.

Исправлены валидатор геометрии (0.1/0.5mm не равно0.3/0.3mm) и ложный threshold test (нулевая S раньше падала INVALID_VALUE). Родитель повторил15 tests. Пять реальных каналов получили geometry connectivity audit; split/multireference/bridge/fence coupons созданы, их EM-приёмка не завершена. Старый DC shared-return prototype:8 DC passes/24 EM unavailable — NO-GO для EM, не развивать как замену.

### B1 — общая геометрия

[Отчёт](p7-b-geometry-results.md). Whole-source STM32 annulus/coordinate-range blocker исправлен общей конформной dyadic-сеткой в пределах pinned i_overlay. Исходные guards сохранены. Geometry preparation85.156s, private531222528B,844 source mappings/89 barrels/93 holes. Это не magnetic solve. Родитель повторно проверял focused Rust/TS regressions; не устраняет отдельно принятую ранее редкую погрешность аппроксимации отверстия у предела допуска.

### B2→C1 — реальное совместное сечение

[Итоговый принятый отчёт](p7-b2-c1-integration-results.md), [задание исправлений](next-task-p7-b2-c1-integration-fixes.md). Первый B2 проверял всю ширину платы и дал0 candidates на всех реальных случаях; это не было принято. Первый C1 терял lengthMm в adapter и позволял одинаковые requests объявлять «уточнениями»; это исправлено.

Теперь finite window проходит полный contour/event scan; исходная медь/острова/внешние обходы сохраняются. Width/air/mesh оцениваются реальными holdouts к одной canonical final конфигурации. Board adapter передаёт length, field cache переиспользуется при другой длине. Родитель повторил49 tests/typecheck, включая2 actual-native adapter tests.

STM32 U5→J5:4 candidates, один автоматически выбранный участок tracks[56]/segment/0 длиной0.2533995509mm, origin[22.0637472868,5.6639]mm. Две TOP сигнальные координаты (tracks45/56), физическая BOTTOM заливка region233/island1. Source board hash0159c43d43b59df4f59fb943d59f0d841d17ca38d6d5a7d5576920a4aee3cce3. Сохранены endpoints/ветви D5/JP2. Width/air/mesh max1.1804/1.9640/0.4151%. Packet `debugging/p7-closure/1793436289092789600`: cold geometry+adapter11.042s, warm adapter3.6812s, sampled private601235456B. NanoScope/Thunder пока0 candidates, с явными местными copper/bore events.

Это условная **локальная PEC C/C0/L/S** приёмка, не whole-channel, не независимая ошибка STM32 и не finite-R земли. Формулы `1/(sigma*w*t)`/internal-L усечённой широкой земли этим результатом не обоснованы. Первоначальный producer addon f0cd8d66… заменён D1-агентом; повторно проверить нужные guards на текущем build при его ревью.

## Последний D1: завершён агентом, РЕВЬЮ ЕЩЁ НЕТ

Полный [отчёт D1](p7-d1-coupled-assembly-results.md) — источник деталей/команд. Задание: два сигнала, одна конечная физически связанная опора с внутренней щелью и реальными медными обходами; compare joint-volume с section+local/P6, cut shift, magnetic-sensitive observables, direct DSL. Фиксированные новые slot/continuous fixtures в `tests/fixtures/analysis-p7-joint-section/`. Внутренний four-port и публичный from/to two-port — разные постановки, не смешивать.

**По отчёту агента**, результат NO-GO:

| Проверка | Сообщённый результат |
| --- | --- |
| Joint magnetic patch1→2 |10.2046% sensitivity, выше2%; сам reference не принят |
| Joint domain2→4mm |0.11162%, проходит только эту численную проверку |
| Slot hybrid vs joint magnetic energy |11.0419%, выше8%; это не установленная физическая ошибка из-за непринятого reference |
| Сдвиг разрезов |3.7795% energy, выше2% |
| Continuous hybrid vs joint |15.2847% energy, тоже отказ |
| Recovered ток через обходы щели |0.41628A против electrode balance0.999554A; на continuous интегратор0.999565 против0.999566A |
| Реальный cold direct DSL final build |160.046s, private595259392B; штатный CHANNEL_SPATIAL_CONVERGENCE по magnetic3.4463%; старые S/loss/delay screens прошли бы |

Агент изменил `native/analyze/src/electrical/coupled/frequency.rs`: descriptor S через существующий loaded solve вместо плохо обусловленного unloaded-Z промежуточного вычисления, intrinsic magnetic/electric energy Gram recovery и finite guards. Z/Y старые. TS: hole events и обязательный magnetic-energy convergence gate. Это **не проверено родителем**. Агент сообщает native:build/30 native tests/60 TS tests/typecheck/build и независимый C1 control pass. Unit tests не закрывают перечисленные EM отказы.

Текущий addon SHA256 (проверен при подготовке handoff): `f15e8aee3c87da0847902c571199a9bc2eaa5f116bf3bb957f686fb9d727ee73`. В отчёте field-cache core447637e389a72800cde469649a204acef4edae8d3d7ed82a2a7d27ad6b433b29. Старый промежуточный matched build2127f6ab… и f0cd8d66… нельзя приписывать нынешним cold результатам.

Ключевые artifacts: `debugging/p7-d1-replay-hardened/`, `debugging/p7-d1-replay-final/`, исходный joint packet1793437157149686200, reduced reproducer1793437346407902100/minimal-assembly-request.json, final cold DSL1793443138032170900, final C1 control1793443484529766900. Cold hybrid на малом coupon~149s, restart-warm~1.16s; активные factors~160MB. Это не доказательство уменьшения памяти большой STM32.

Гипотеза агента: scalar face potential/total current не передают форму возвратного тока и межблочную field energy; предлагается исследовать trace moments/согласованное energy ownership. **Это гипотеза/направление, не принятое архитектурное решение и не разрешение сразу писать новый solver.** Сначала отделить проблемы reference/current recovery от недостатка интерфейса и проверить новые transport/energy изменения. Не «исправлять»11% эмпирической подгонкой или ослаблением guards.

## Где искать код

- `src/analyze/analysis-dsl/`, `run-analysis-dsl.ts`, `execute-problem.ts` — DSL/worker/native/cache.
- `board-channel-plan.ts`, `route-partition.ts`, `run-corridors.ts`, `geometry-snapshot.ts` — маршрут, области/источники, B2/C1 selection.
- `section-domain.ts`, `segmented-channel.ts` — refinement, board adapter, section descriptors/P6; `board-channel-executor.ts` — channel admission (D1 изменил magnetic evidence).
- `native/analyze/src/extraction/board_section.rs`, `polygon_section.rs`, `section_cache.rs`; `extraction/local/` — полевые методы/mesh/reduction/recovery.
- `native/analyze/src/electrical/coupled/` — P6 assembly/waves/energy.
- `scripts/verify-p7-{closure-*,wide-*,joint-section.ts}`, `tests/analyze-*`, fixtures `analysis-p7-closure/`, `analysis-p7-wide-section/`, `analysis-p7-joint-section/`.

## Workflow, ограничения и состояние Git

Пользователь хочет архитектора/валидатора в основном чате и реализацию дешёвыми агентами **GPT-6.1-sol medium**. Задания в читаемых markdown-файлах: границы, зависимости, tests, реальные платы, plots и приёмка. Не запускать review-агентов просто ради заполнения slots. Независимые реализации распараллеливать только с разделённым владением. Один heavy run одновременно; не собирать addon параллельно численным измерениям.

**После выдачи/запуска задания сразу остановиться, агента не ждать.** Пользователь сам возвращается по завершении. Экономить токены: никаких постоянных polling/чтений агента. Нет запроса автоматически запускать новую работу в этом handoff-ходе.

Авторизация на обычные reversible fixes/test/review есть; не спрашивать разрешение на каждую операцию. Когда пользователь просит «если устраивает — коммить», формат Conventional Commits, только проверенный scope. Нельзя объявлять pass по аварии/missing evidence; timeout/unsupported/unverified показывать честно. Не повторять дорогие неизменные отказы и положительные эталоны без конкретной причины. Сохранённые evidence/replays маркировать reused, не cold. Графики просматривать.

Git HEAD при handoff: `7258677 chore(analyze): retain magnetic memory audit and reject failed candidates`. Много накопленных modified/untracked файлов, включая работающий код и отчёты; новых коммитов в последних A–D1 проходах не было. Нельзя reset/clean/stage-all «для уборки», удалять user/debugging, особенно untracked `native/analyze/debugging/`. Сначала инвентаризация diff/владения; непринятый D1 не коммитить как завершённый P7. Свежую `git status`/изменения проверять, не полагаться на список этого документа.

## Готовая стартовая инструкция следующему агенту

«Прочитай docs/experimental/analyze/chat-handoff-2026-10-31.md и связанные текущие plan/D1 task/report. Продолжаем разработку PCB-анализатора. Последний D1-агент завершился с NO-GO; его результат и native transport/energy изменения пока не ревьюились. Сначала проверь код и сохранённые доказательства, отдели несходимость joint reference/recovered current от проблемы section+local интерфейса. Не запускай новые solver campaigns/агентов и не повторяй тяжёлые runs без причины. Сохрани всю незакоммиченную работу; объясни пользователю вывод и конкретный следующий шаг. P7/P8/P11 не объявляй закрытыми.»
