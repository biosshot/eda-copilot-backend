# Post-place / Refiner: массовые оценки на GPU

Дата: 2026-09-30. **Статус: план; реализация не начата.** Выполнять после [Board Packager roadmap](BOARD_PACKAGER_GPU_ROADMAP.md). Техническая обязательная зависимость — завершённый `compute`, описанный в [GPU API](GPU_INFRASTRUCTURE_API.md) и [результатах инфраструктуры](GPU_INFRASTRUCTURE_RESULTS.md). Этот модуль не требует board GPU kernels: готовую board реализацию используем как потребителя общего runtime и сохраняем её регрессионные проверки.

Перед реализацией проверить актуальные исходники, незакоммиченные изменения и hash addon. На дату написания общая инфраструктура завершена в рабочем дереве поверх `6d1f9c8`; один этот commit не является её полной базой. После предыдущей задачи заново сохранить baseline, не сравнивать изменившийся весь pipeline только с историческими числами.

## Результат и границы

Создать **один GPU evaluator общей post-place цели**, используемый native refiner в локальной доводке блоков и финальной доводке платы. Основной перенос — массовые независимые scores относительно одного текущего состояния; не дублировать реализацию для этих двух callers. Измерить отдельно scorer, полную итерацию, полный native refinement и суммарную доводку в цикле сборки платы.

Rust/CubeCL, GPU F64, CPU F64, готовые runtime/lease/scratch/numerics, один `.node`, ориентир до 20 МБ. F32 для всех CPU/GPU floating вычислений — отдельная будущая задача без срока. Состав moves, ограничения, разрешения на fixed components, route policy и качество не упрощаются ради ускорения.

CPU сохраняет генерацию moves, дешёвую проверку допустимости, принятие одного изменения за итерацию и Micro-A*. Это осознанная начальная граница: legality в refiner — отдельный алгоритм с идентичностями нарушений; сначала измерить его долю. Если после переноса score именно geometry/generation ограничивают полный цикл, добавить обоснованный этап в этот roadmap, а не объявлять их заранее быстрыми. Массовые члены **score** не оставлять CPU дополнением к неполному GPU score.

## Карта кода и единая реализация

| Источник | Назначение и граница |
|---|---|
| `native/pcb-board-packer/src/post_place.rs::score` | CPU эталон общей цели: MST, длины, crossings, pad hits, distances, clearances, fixed penalties, edges, paths; будущий единый GPU scorer |
| `post_place_refine.rs::score` | Сейчас заново собирает `PostPlaceScoreProblem` из `World`; для массового GPU пути нужен resident topology и компактные изменения poses, без полной пересборки/копирования всех nets/obstacles для каждого кандидата |
| `evaluate`, `iteration`, `solve` в том же файле | Доменный GPU Engine и batches, CPU validity/route stages, прежние candidate IDs и стабильный выбор лучшего move |
| `post_place_refine.rs::diagnostics` | Сохранить финальные диагностические проверки и разрешения на fixed changes; учитывать их время в полном вызове, не вырезать ради benchmark |
| `src/pcb-layout/pcb-auto-place-v2/block-post-refiner.ts` | Тот же native refiner с `routingMetric=geometric`, 8 passes и 2 с; острова остаются rigid, scopes и allowed rotations сохраняются |
| `post-place-refiner.ts`, `post-place-budget.ts`, `native/encode-post-place-refine.ts` | Финальный route-aware refiner, adaptive iterations и штатный 30-секундный бюджет; сохранить один NAPI вызов на полный поиск |
| `globalPostPlaceScore`, `scorePostPlace` | Существующий scalar контракт сохраняется. Для одиночных маленьких вызовов допустим CPU; не выполнять отдельный GPU dispatch на каждый TS вызов автоматически |
| `micro_router/comparison.rs` | Существующие baseline, maximum improvement bound, feasibility и маршруты остаются CPU. Jobs внутри сравнения резервируют `TemporaryRoutes` последовательно, это не независимый GPU batch |

Имена предполагаемых новых файлов — `post_place/cubecl.rs`, `post_place/gpu_kernels.rs` или аналогичные внутренние подмодули. GPU формулы принадлежат post-place, а не board packer и не `compute`. Общие геометрические/численные helpers выделять только при совпадении правил; block MST или pad cache не считать автоматически эквивалентными post-place.

## Контракт batches и выбора результата

1. **Замороженное состояние итерации.** Генерировать прежний список candidates на CPU с прежними ID/порядком. Оценивать batches относительно одного `current`; только после выбора move изменять состояние и инвалидировать зависимые resident caches. Нельзя объединить последовательные итерации или принять несколько независимых на вид moves одновременно.
2. **Legality сохраняет идентичности.** Текущая проверка разрешает кандидата, когда множество его нарушений — подмножество исходных нарушений для changed components. Сравнение только hard count недостаточно: нельзя заменить старое нарушение новым. Сохранить locked/explicit groups, board/regions/holes, clearance, side/through-hole и allowed rotations. Отфильтровывать CPU-invalid кандидатов до дорогостоящего score, без массового CPU пересчёта score.
3. **Представление данных.** Один Engine хранит component orientation templates, pads/owners/layers, net topology, hints, paths и стабильную геометрию. Batch передаёт changes/poses/IDs, а не полную копию `World`. Состав derived distance/clearance/path terms должен соответствовать текущему encoder; отсутствие target не превращать в выдуманную нулевую координату. Перенести их массовое вычисление/материализацию вместе с GPU score.
4. **Полная цель.** Сохранить детерминированный MST со своим EPS/tie-break, линейный и квадратичный wirelength, weighted crossings, правила shared endpoints, pad-hit layer/net exemptions, internal owner и dedup физического pad, distance min/max, clearance gap, fixed penalties, edge и signal-path topology. `fixed_penalties` не обязательно постоянны между moves: проверить зависимости от слоя/poses. Таблица всех terms и условий нулевого вклада обязательна до интеграции.
5. **Параллельность.** Многие candidates/workgroups одновременно; по возможности независимые проверки внутри кандидата распределяются по lanes. Последовательные зависимости MST сохраняются. Batches ограничены памятью и временем dispatch; не удерживать весь набор полных worlds и не создавать runtime на каждый native thread.
6. **Геометрический режим.** После полного GPU score можно вернуть лучший допустимый move/необходимые scores, сохранив `min_delta`, EPS и stable order. Ускорение не достигается уменьшением числа исходных кандидатов.
7. **Route-aware режим.** Нельзя отдать Micro-A* только произвольный top-K по geometric score: кандидат может выиграть исключительно за счёт routability. Сохранить `comparison::prepare`, `maximum_improvement` bound с прежним численным margin, feasibility order и final effective improvement. Возвращать компактные scores/ID всех кандидатов, которые нельзя безопасно отсечь, либо эквивалентную доказанную фильтрацию; небольшой shortlist не гарантируется. Нет полезной границы — маршрутную проверку выполняют все оставшиеся кандидаты.
8. **Порядок.** У serial пути есть incumbent bound, у parallel групп свой baseline cache. GPU batching не должен менять winner при законченной одинаковой работе; маршрутные результаты объединяются по исходным candidate IDs, не по порядку завершения workers. CPU route работа выполняется вне GPU mutex.
9. **Совместный runtime.** Собственные `ScratchKey` layouts, `Requirements`, resident Engine. Не хранить scratch handles между `with_session` операциями. Смена poses, topology, pad ownership/layers, весов и movable obstacles корректно инвалидирует affected caches, в том числе когда net сам не двигался. Busy/unsupported не отключает соседние consumers.

## Timeout, CPU fallback и точность

Предлагаемый доменный flag — `PCB_POST_PLACE_BACKEND=cpu|cubecl|auto` для refiner и его общего scorer; это планируемое имя. Сохранить существующие thread settings и scalar NAPI. `cpu` не требует GPU, `cubecl` не обходит guards, `auto` выбирается по измеренным batch/geometry размерам и cold/ready runtime. Возможность держать scalar standalone score на CPU не должна оставлять массовый refiner на CPU незаметно для отчёта.

При неподдержанном входе/no-device/disabled/busy весь native refinement выполняется CPU. При ошибке GPU после начала поиска полностью отбросить GPU moves, scores, caches и placements, повторить `refinePostPlacement` с **исходным RefineProblem и исходными placements**, без GPU retry. Не продолжать CPU с уже улучшенной GPU расстановки. Для самостоятельного read-only `scorePostPlace` граница повтора — тот же исходный score request. Некорректный вход сохраняет прежнюю ошибку валидации.

**Бюджет CPU повтора согласован пользователем 2026-09-30:** после GPU failure CPU получает исходный `timeoutMs` заново. Потерянное GPU время отражается отдельно, полное wall time включает обе попытки и может превысить обычные 2/30 с. Например, сбой после 20 с финальной доводки допускает ещё до 30 с CPU поиска, плюс накладные расходы и cooperative timeout overshoot. Повтор ровно один, только на CPU; оставшееся от GPU попытки время не ограничивает его бюджет.

В нормальном режиме сохраняется прежний cooperative timeout: init/upload/dispatch/readback входят в время вызова, не выдавать GPU дополнительный скрытый бюджет. Уже запущенный kernel может закончиться после deadline, поэтому ограничивать batch sizes и измерять overshoot; не обещать прерывание dispatch по таймеру. Не возвращать несогласованную или наполовину применённую расстановку. Сохранять текущие правила выбора среди успевших завершиться кандидатов.

CPU F64 остаётся reference. Проверять hard decisions, геометрию, отдельные score terms и reduction, shortlist/selection. Численный допуск score не означает разрешение изменить победителя. На совпадающей законченной работе сохранять moves/poses и route decisions; около EPS/clearance/MST ties добиваться корректной арифметики или явно ограничивать поддержку. Production не должен полностью пересчитывать все GPU candidates на CPU.

## Два разных вида проверки

| Режим | Что фиксируем | Что доказывает |
|---|---|---|
| Одинаковая работа | Сохранённые worlds/candidate batches и IDs; завершённые одинаковые итерации; одинаковые threads/route rules. Для тяжёлых cases отдельный bench replay без ограничения production timeout, не изменение production API | Численная корректность, одинаковый выбор, скорость одинакового объёма оценок и полного законченного цикла |
| Штатное время | Исходный input/placements, стандартные 2/30 с и iteration limits, CPU/GPU отдельно | Реальное качество, checked candidates, принятые moves, completed passes, route feasibility, stop reason и wall time |

Если оба запуска упираются в 30 с, не писать «ускорение в N раз» по отношению одинаковых wall times. GPU может выполнить больше работы за тот же бюджет. Не требовать идентичных outputs между двумя разными недовыполненными поисками, но сохранять hard invariants и проверять качество независимым CPU scorer/validator вне таймера. Если same-budget качество ухудшилось, выяснить причину, не скрывать это увеличением числа кандидатов. Timed CPU replay также может завершиться на другой границе; exact comparison выполнять на законченных детерминированных cases.

## Этапы

### 1. Baseline и полная карта цели

- [ ] Прочитать root/native/PCB `AGENTS.md`, GPU API, `post_place.rs`, весь путь refiner и оба TS callers. Найти существующие score/cache helpers, tests, capture и benchmarks.
- [ ] Зафиксировать реальные исходные **native RefineProblem** и CPU outputs для нескольких локальных блоков (включая тяжёлый при наличии), финальных плат Telemetry/esp32c3/ESPower и тяжёлого captured случая. Сохранить local geometric и final route-aware случаи, valid references, inputs/source/addon hashes.
- [ ] Разделить encoding/world update, generation, geometry, MST/score terms, route baseline/compare, diagnostics, mutex и total wall. Суммарные worker milliseconds не выдавать за wall time. Сохранить finished и timeout cases.
- [ ] Составить матрицу score terms/guards и зафиксировать уже согласованный полный CPU replay с новым исходным бюджетом в проверках. Не переносить лимит 20 primitives из block scorer: размеры post-place определяются его собственными данными и GPU limits.

**Выход:** реальный baseline, объём выигрышной массовой работы и полный контракт поддержки.

### 2. Единый GPU scorer и численная проверка

- [ ] Реализовать resident representation и полный batched evaluator на сохранённых candidates, включая подвижные pads/obstacles и derived hints/path terms. Не копировать CPU scorer отдельно под каждого caller.
- [ ] Сравнить каждый term/score с CPU, MST endpoints/ties, crossings и pad-hit dedup, near-boundary/large-coordinate cases, signed/zero weights в пределах действующего контракта, empty/singleton nets. Проверить реальные GPU kernels, не только host версию helpers.
- [ ] Проверить разные batch/chunk sizes, порядок candidates, кеши при moves/rotations/swaps, изменившийся foreign pad при неподвижном net и общий scratch с другими consumers. Сопоставить pruned/unpruned результаты при добавлении отсечения.

**Выход:** один корректный полный score kernel pipeline с известной стоимостью upload/compute/readback и resident памяти.

### 3. Интеграция обоих режимов refiner

- [ ] Подключить batches сначала к geometric режиму, затем к route-aware; сохранить validity subset, группировку changed components, baseline caches, bounds и stable winner.
- [ ] Сохранить route-only улучшения: существующий тест «swap can be selected purely by Micro-A* routability» обязателен. Проверить explicit fixed permissions, rotations/swaps, default layer и opposite-side pads.
- [ ] Проверить полные local и final native calls: initial score, generation, все завершённые passes, diagnostics и итоговый результат. Подобрать `auto` и batches по полному времени, а не только kernel benchmark.
- [ ] Оставить scalar `scorePostPlace` совместимым. Подключать GPU к независимым массивам score requests только там, где существующий caller действительно имеет batch и это окупается; TS alignment/portfolio алгоритмы и новый NAPI batch контракт не являются обязательной частью этого roadmap.

**Выход:** local и final refinement используют один scorer, route semantics и budgets сохранены; преимущества/CPU остаток измерены отдельно.

### 4. Восстановление, timeout и приёмка

- [ ] Проверить no-device/disabled/busy/unsupported/unsafe, failure в batch, после принятого move и перед diagnostics; исходный CPU replay без смешивания poses/moves двух попыток, профиль обеих попыток.
- [ ] Проверить timeout 0/малый/штатный, истечение перед/между batches, bounded overshoot, корректное состояние scratch после отказа, no-improvement и iteration-limit завершения.
- [ ] Проверить 1/2/4 разрешённых native workers, mixed block/board/post-place workload и разные процессы с OS lease. Одна сессия, отсутствие лишних context и CPU oversubscription, сохранение готовых block/board результатов.
- [ ] Выполнить `npm run native:build`, Rust tests с GPU и `--no-default-features`, `npm test -- pcb-post-place`, `npm test -- pcb-block`, `npm test -- pcb-board`, `npm run typecheck` и затронутые score/geometry tests. Проверить addon size/imports и изолированные GPU/CPU вызовы.
- [ ] Провести same-work series: первый запуск и минимум три тёплых повтора на одинаковой машине без внешней GPU нагрузки. Отдельно production-budget quality runs и полный цикл нескольких плат с зафиксированными остальными backends; оценить local refine aggregate и final refine отдельно.
- [ ] Записать stage speedups, полное время, candidates/passes, route metrics, validation/placementOk, hashes, fallback coverage и limitations; обновить changelog и закрыть пункты только с evidence.

**Приёмка:** полная массовая score цель на GPU, сохранённый выбор на одинаковой законченной работе, корректный replay и hard invariants; измеримое улучшение полного refinement либо объёма работы за тот же бюджет без скрытого ухудшения качества. Один ускоренный scorer без проверки целого refiner не закрывает задачу. Не обещать заранее множитель и не исправлять unrelated placement bugs через смену reference.

## Инструменты, evidence и последующие задачи

Переиспользовать `scripts/benchmark-post-place.ts`, `benchmark-native-post-place.ts`, `benchmark-post-place-boundary.ts` и [capture инфраструктуру](../../../pcb-layout-debugging.md). У существующих benchmarks есть искусственные ограничения состава групп/числа итераций, а boundary benchmark не измеряет полный refiner: проверить и явно записать условия. Текущий debug capture описывает block/board; exact refiner capture добавить через существующий механизм при реализации, не утверждать, что он уже есть. Native solve cache и validation CPU rescoring выключить в performance runs.

Evidence — отдельный `debugging/post-place-gpu-<date>/`; выводы и команды — будущий `POST_PLACE_GPU_RESULTS.md`. Сохранять входные placements до каждой стадии, чтобы сравнение scorer/refiner не зависело от новых board outputs. Полная интеграционная проверка отдельно использует естественную последовательность стадий.

За пределами этого плана: GPU Micro-A*, перенос TS controllers, отдельные GPU alignment/portfolio/island solvers и F32. После приёмки смотреть новый профиль: не создавать следующий roadmap только потому, что в коде встречается слово `score`.
