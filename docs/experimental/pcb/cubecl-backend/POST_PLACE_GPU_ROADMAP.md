# Post-place / Refiner: массовые оценки на GPU

Дата: 2026-09-30. **Статус на 2026-10-01: реализован и принят в измеренной области; результаты и ограничения — в [отчёте](POST_PLACE_GPU_RESULTS.md).** Выполнять после [Board Packager roadmap](BOARD_PACKAGER_GPU_ROADMAP.md). Техническая обязательная зависимость — завершённый `compute`, описанный в [GPU API](GPU_INFRASTRUCTURE_API.md) и [результатах инфраструктуры](GPU_INFRASTRUCTURE_RESULTS.md). Этот модуль не требует board GPU kernels: готовую board реализацию используем как потребителя общего runtime и сохраняем её регрессионные проверки.

База реализации — Board Packager commit `c17b564`; исходный addon и hashes сохранены в `debugging/post-place-gpu-2026-10-01/baseline/`. Финальный native build, исходники и addon связаны через `build-final/manifest.json`. Сам commit `c17b564` ещё не содержит эту реализацию Refiner.

Уточнение пользователя 2026-10-01: те же правила тестирования — один CPU и один GPU проход на тест/версию; подходящие сохранённые CPU inputs/results использовать повторно. Серии медиан и обязательные тёплые повторы отменены. Повторять лишь после изменения кода или обнаруженной ошибки.

## Результат и границы

Создать **один GPU evaluator общей post-place цели**, используемый native refiner в локальной доводке блоков и финальной доводке платы. Основной перенос — массовые независимые scores относительно одного текущего состояния; не дублировать реализацию для этих двух callers. Измерить отдельно scorer, полную итерацию, полный native refinement и суммарную доводку в цикле сборки платы.

Rust/CubeCL, GPU F64, CPU F64, готовые runtime/lease/scratch/numerics, один `.node`, ориентир до 20 МБ. F32 для всех CPU/GPU floating вычислений — отдельная будущая задача без срока. Состав moves, ограничения, разрешения на fixed components, route policy и качество не упрощаются ради ускорения.

CPU сохраняет генерацию moves, дешёвую проверку допустимости, принятие одного изменения за итерацию и Micro-A*. Это осознанная начальная граница: legality в refiner — отдельный алгоритм с идентичностями нарушений; сначала измерить его долю. Если после переноса score именно geometry/generation ограничивают полный цикл, добавить обоснованный этап в этот roadmap, а не объявлять их заранее быстрыми. Массовые члены **score** не оставлять CPU дополнением к неполному GPU score.

## Карта кода и единая реализация

| Источник | Назначение и граница |
|---|---|
| `native/pcb-board-packer/src/post_place.rs::score` | CPU эталон общей цели: MST, длины, crossings, pad hits, distances, clearances, fixed penalties, edges, paths; CPU reference для единого GPU scorer |
| `post_place_refine.rs::score` | CPU/scalar строит `PostPlaceScoreProblem` из `World`; массовый GPU путь использует resident topology и компактные changes/poses без полной пересборки nets/obstacles |
| `evaluate`, `iteration`, `solve` в том же файле | Доменный GPU Engine и batches, CPU validity/route stages, прежние candidate IDs и стабильный выбор лучшего move |
| `post_place_refine.rs::diagnostics` | Сохранить финальные диагностические проверки и разрешения на fixed changes; учитывать их время в полном вызове, не вырезать ради benchmark |
| `src/pcb-layout/pcb-auto-place-v2/block-post-refiner.ts` | Тот же native refiner с `routingMetric=geometric`, 8 passes и 2 с; острова остаются rigid, scopes и allowed rotations сохраняются |
| `post-place-refiner.ts`, `post-place-budget.ts`, `native/encode-post-place-refine.ts` | Финальный route-aware refiner, adaptive iterations и штатный 30-секундный бюджет; сохранить один NAPI вызов на полный поиск |
| `globalPostPlaceScore`, `scorePostPlace` | Существующий scalar контракт сохраняется. Для одиночных маленьких вызовов допустим CPU; не выполнять отдельный GPU dispatch на каждый TS вызов автоматически |
| `micro_router/comparison.rs` | Существующие baseline, maximum improvement bound, feasibility и маршруты остаются CPU. Jobs внутри сравнения резервируют `TemporaryRoutes` последовательно, это не независимый GPU batch |

Реализация — `post_place/cubecl.rs` и `post_place/gpu_kernels.rs`. GPU формулы принадлежат post-place, а не board packer и не `compute`. Общие геометрические/численные helpers выделять только при совпадении правил; block MST или pad cache не считать автоматически эквивалентными post-place.

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

Реализованный доменный flag — `PCB_POST_PLACE_BACKEND=cpu|cubecl|auto` для native refiner, default `auto`; scalar standalone score остаётся CPU. Сохранить существующие thread settings и scalar NAPI. `cpu` не требует GPU, `cubecl` не обходит guards, `auto` выбирается по измеренным batch/geometry размерам и cold/ready runtime. Возможность держать scalar standalone score на CPU не должна оставлять массовый refiner на CPU незаметно для отчёта.

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

## Закрытые этапы и evidence

Исходные проверки ниже уточнены по фактически выполненному объёму. Широкие
неизмеренные матрицы перечислены отдельно; они не объявлены пройденными.

### 1. Baseline и полная карта цели

- [x] Прочитаны root/native/PCB правила, GPU API, CPU scorer/refiner и оба TS callers; переиспользованы существующие encoding, geometry, route comparison и capture.
- [x] Сохранены точные native inputs и CPU outputs: Telemetry/ESPower final, три USB local, FPGA capacitor group и полный 19-primitive FPGA local. esp32c3 проверен полным циклом с CPU refiner в auto. CPU references повторно не пересчитывались для финального GPU build.
- [x] Зафиксированы generation, legality/geometry, score, route baseline/compare, evaluation wall и полный native wall; runtime сообщает encoding/operations/mutex/workspace. Worker sums не выданы за wall; законченные и timeout cases разделены. Отдельный hardware timestamp каждого kernel не добавлялся.
- [x] В отчёте приведена матрица всех score terms, guards и полный CPU replay с новым исходным бюджетом.

### 2. Единый GPU scorer и численная проверка

- [x] Resident topology/templates, compact poses/changes, полный score: geometry, MST, crossings, pad hits, distance/clearance/fixed/edge/path terms; одна реализация для обоих callers.
- [x] Реальные GPU kernels сверены с независимыми CPU geometry/MST endpoints/lengths, отдельными hint/path terms и aggregate score. Полные ESPower и FPGA local проходят проверку каждого допустимого кандидата; focused test покрывает MST ties и составную цель. Negative net weights, unsupported layers, unsafe numbers и duplicate nets уходят на CPU.
- [x] Проверены chunks 1/17/128, workers 1/2/4, полный выбор после moves/swaps, shared runtime и независимость buffers. Нового geometric top-K или приближённого pruning не вводилось.

### 3. Интеграция обоих режимов refiner

- [x] Geometric и route-aware используют единый evaluator; CPU violation subset, changed-component groups, caches, bounds и stable reduction сохранены.
- [x] Тест «swap can be selected purely by Micro-A* routability» отдельно пройден на настоящем GPU. Проверены fixed permissions, rotations/swaps и opposite-side pads.
- [x] Измерены полные native calls с initial/final работой. Telemetry: 17.634 -> 12.887 с (1 worker, cold), 7.053 -> 5.649 с (4 workers, ready GPU). На 6 workers CPU быстрее: auto сохраняет CPU.
- [x] Auto допускает >=154 components, >=660 pads, >=235 MST segments, iterations>0, timeout>=2000 ms и 1 worker либо <=4 с ready runtime. Малые local calls остаются CPU. Scalar score и NAPI совместимы; массового CPU rescoring в production GPU пути нет.

### 4. Восстановление, timeout и приёмка

- [x] Проверены disabled/no-device/busy, domain guards, ошибки batch/after_move/diagnostics. Отбрасывается весь GPU результат, CPU повторяет исходный input с полным timeout; runtime/lease освобождается.
- [x] Проверены zero/small timeout, законченные no-improvement/iteration-limit calls, production 2-second geometric budget probe. За одинаковый бюджет GPU принимает 6 улучшений вместо 3, CPU rescoring подтверждает лучший score и hard validity. Cooperative overshoot сохранён и измерен.
- [x] 1/2/4 workers, межпроцессный lease и освобождение после failure, block -> board -> block regression, полный ESPower с block/board/post-place на одном runtime. Producer и route workers разделяют CPU permits.
- [x] Native build; Rust GPU 64 pass/2 ignore, CPU-only 60 pass/1 ignore; 29 focused tests + 1 forced GPU route-only + 3 Board/block regressions + 1 guards/no-device test. Typecheck и package build успешны. Addon 20,088,320 bytes (20.088 MB / 19.158 MiB), прежние системные imports, один `.node`.
- [x] Один CPU/GPU проход на каждую версию/конфигурацию с сохранёнными CPU references; повтор GPU только после изменения реализации. Полные ESPower и esp32c3 имеют placementOk=true, точные placements и побайтно одинаковые SVG с сохранёнными references. Принудительный GPU на малом ESPower не ускоряет весь pipeline; это явно отражено в отчёте.
- [x] Обновлены results/changelog, сохранены hashes, stage profiles, failures и ограничения. Exact capture/replay добавлен через существующий механизм, без нового production API.

### Непроверенные расширения области

- [ ] Полный TypeScript цикл Telemetry/PortableScope с новым Refiner и отдельный 30-second timeout quality run на тяжёлой route-aware плате. Эти дорогие дополнительные проходы не выполнялись; native Telemetry и два полных меньших примера не подменяют их.
- [ ] Исчерпывающая матрица больших/пограничных чисел, empty/singleton nets и всех signed hint combinations на GPU. Проверенные реальные inputs и guards не являются доказательством всех комбинаций.
- [ ] Производительность и аппаратный F64 capability fallback на других GPU. No-device проверен недоступным Vulkan driver на текущей машине, а не физическим устройством без F64.

**Приёмка:** полная массовая score цель на GPU, сохранённый выбор на одинаковой законченной работе, корректный replay и hard invariants; измеримое улучшение полного refinement либо объёма работы за тот же бюджет без скрытого ухудшения качества. Один ускоренный scorer без проверки целого refiner не закрывает задачу. Не обещать заранее множитель и не исправлять unrelated placement bugs через смену reference.

## Инструменты, evidence и последующие задачи

Переиспользовать `scripts/benchmark-post-place.ts`, `benchmark-native-post-place.ts`, `benchmark-post-place-boundary.ts` и [capture инфраструктуру](../../../pcb-layout-debugging.md). У существующих benchmarks есть искусственные ограничения состава групп/числа итераций, а boundary benchmark не измеряет полный refiner: проверить и явно записать условия. Debug capture теперь сохраняет точные `refine` inputs/outputs для local/final calls; `scripts/debug-pcb-layout.mjs` воспроизводит их. `scripts/experiment-post-place-gpu.mjs` делает один полный native проход и сравнивает с сохранённым CPU reference. Native solve cache и validation CPU rescoring выключить в performance runs.

Evidence — отдельный `debugging/post-place-gpu-<date>/`; выводы, условия замеров и проверки — [POST_PLACE_GPU_RESULTS.md](POST_PLACE_GPU_RESULTS.md). Сохранять входные placements до каждой стадии, чтобы сравнение scorer/refiner не зависело от новых board outputs. Полная интеграционная проверка отдельно использует естественную последовательность стадий.

За пределами этого плана: GPU Micro-A*, перенос TS controllers, отдельные GPU alignment/portfolio/island solvers и F32. После приёмки смотреть новый профиль: не создавать следующий roadmap только потому, что в коде встречается слово `score`.
