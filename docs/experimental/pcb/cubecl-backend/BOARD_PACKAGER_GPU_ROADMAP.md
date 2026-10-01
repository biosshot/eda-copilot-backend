# Board Packager: массовые вычисления на GPU

Уточнение пользователя 2026-10-01: работа возобновлена для устранения недостатков GPU pipeline. Проверять по одному проходу на тест, CPU references/замеры переиспользовать; серии медиан и три тёплых повтора ниже больше не требуются. Повторять проверку только после изменения реализации или для исправления найденной ошибки.

Дата: 2026-09-30. **Статус на 2026-10-01: Реализация и оптимизация Board Packager завершены в проверенной области; по указанию пользователя переходим к Post-place / Refiner. Недостатки GPU pipeline исправлены. Полный native Telemetry ordinary + aligned ускорен с 341,976 до 162,887 с (2,10x CPU); результаты точные. Полный цикл ESPower прошёл с неизменной расстановкой. Новые замеры — по одному проходу; широкая приёмка полного TS pipeline остаётся отдельно открытой.** Ход работы и evidence — [результаты](BOARD_PACKAGER_GPU_RESULTS.md). Документ подготовлен вместе с [Post-place / Refiner roadmap](POST_PLACE_GPU_ROADMAP.md). Выполнять последовательно: сначала Board Packager, затем Post-place / Refiner. Каждый модуль зависит от готовой общей инфраструктуры, а не от реализации второго roadmap.

Основа: завершённые [GPU infrastructure roadmap](GPU_INFRASTRUCTURE_ROADMAP.md), [API](GPU_INFRASTRUCTURE_API.md), [результаты](GPU_INFRASTRUCTURE_RESULTS.md). При начале реализации проверить актуальное дерево: на момент написания завершённое выделение `compute` находится в локальных изменениях поверх `6d1f9c8`, поэтому этот commit сам по себе не воспроизводит новый runtime. Сохранить исходники, addon hash и CPU/GPU baseline; чужие изменения не откатывать и не включать молча в новую работу.

## Результат и границы

Ускорить **полный board packing**: Beam, совместные размещения пар, local improve, repair и финальную оценку. Основная работа — массовые проверки допустимости и вычисление полного геометрического rank кандидатов на GPU. Перенос одного `board_score` без предшествующего CPU обхода hard constraints не завершает задачу.

Используем Rust/CubeCL, GPU F64 и неизменённый CPU F64. Один process-wide runtime, один общий workspace под mutex, OS lease между процессами, та же `.node`; ориентир поставки до 20 МБ. Никаких OpenCL/helper DLL/CPU JIT. Общий F32 перенос всех CPU/GPU floating вычислений остаётся отдельной задачей без срока.

Сохраняем генератор и состав кандидатов, search width, последовательность принятия решений, геометрию, веса, hard constraints, маршрутизацию и точный CPU reference. CPU управляет поиском и обрабатывает существующий маршрутный shortlist; GPU параллельно оценивает многие кандидаты. Не переносим TypeScript orchestration, block internals, post-place/refiner или islands в рамках этого плана.

## Карта исходного кода и границы переноса

Пути ниже относительны корню backend; native функции находятся в `native/pcb-board-packer/src/solver.rs`.

| Место | Существующее поведение и план |
|---|---|
| `solve`, `solve_with_threads`, `expand_states` | CPU контроллер и bounded native threads; объединять независимые оценки одной стадии в batches, сохраняя порядок states и ordinals |
| `position_candidates`, `candidate_hard_count` | Генерация centers, ориентации, fit-to-bounds остаются CPU. Массовый legality filter переносится GPU вместе с оценкой; область правила «если есть legal, оставить только их» сохранить отдельно для каждого исходного вызова генератора |
| `score_positions`, `state_rank`, `board_score` | Полные `hard_count`, `hard_severity`, `score` на GPU; не дополнять каждый GPU score массовым CPU обходом geometry/terms |
| `finish_cheap_candidates`, `rerank_candidates` | Сохранить dedupe и стабильный ordinal, отдельные бюджеты ordinary/alignment (сейчас 32/16), legality filter и существующий `lazy_rank`; GPU возвращает нужный shortlist, CPU выполняет прежние Micro-A* corrections |
| `joint_pair_states` | Отдельные batches двух движущихся primitives; сохранить atomic pair, порядок проверки пары и переходы между одинаковыми глубинами Beam, существующие shortlist 16/4 |
| `local_improve`, `repair_hard_violations` | Использовать тот же GPU evaluator; решения между последовательными moves/passes остаются CPU. Малые разовые контрольные оценки допустимы на CPU; массовый repair не оставлять скрытым CPU scorer |
| `src/pcb-layout/pcb-auto-place-v2/board-solver.ts` | Сохранить полный ordinary solve и дополнительный aligned solve при наличии пар, выбор portfolio и последующий local portfolio; их время учитывать в общем результате |
| `board-packer-engine.ts`, `native/encode-board-problem.ts` | Сохранить NAPI/TS контракт и применение решения. При измерениях выключать native solve cache, чтобы попадание в кеш не выдавалось за GPU ускорение |

Board score — не block score и не post-place score. Не копировать block scorer целиком и не подменять формулы похожими. Переиспользовать `compute::numerics`, runtime и подход compact poses/resident buffers; общие geometry/path helpers выделять только при совпадении семантики и проверке всех существующих потребителей.

## Контракт оценки и данных

- **Полная таблица terms до kernels.** Для каждого активного поля `BoardPackProblem` сопоставить CPU функцию, GPU представление, тест и поведение unsupported. Обязательное покрытие контрольного набора: compound primitives с несколькими components/collision boxes, top/bottom и through-hole, locked, allowed orientations, component clearances, outline и допустимый overflow, obstacles/holes, constraint regions, edge placement, relations/endpoints, signal-path topology, envelope/area/perimeter, оба compactness режима, soft spacing/exempt pairs и soft alignment/orientation.
- **Rank состоит из трёх значений.** Сохранить целочисленный hard count и порядок `(hard_count, hard_severity, score, ordinal)`. `rank_improves` с текущими допусками отличается от обычной сортировки; нельзя заменить оба сравнения одним epsilon comparator. NaN/Inf, переполнение размеров и индексов, небезопасные координаты отклонять до dispatch. Новые численные допуски не вводить ради прохождения тестов.
- **Компактное представление.** Доменный Engine хранит шаблоны ориентаций, component/layer geometry, relations, outline и rules на GPU. Batch содержит poses/ID, ссылку на неизменяемый родительский frame и границы исходных групп. Не отправлять полную геометрию платы для каждого кандидата. Преобразования воспроизводят текущие `translate_primitive`/`rotate_primitive` и места округления, включая повторные преобразования в repair.
- **Параллельность и память.** Многие кандидаты/workgroups выполняются параллельно; внутри больших кандидатов распределять независимые geometry/term проверки по lanes. Размеры batches ограничивать доступной памятью и лимитами dispatch; разбиение не меняет ordinal или результат reduction. Не выделять память пропорционально всем кандидатам всех уровней поиска сразу.
- **Resident и scratch.** Собственные `ScratchKey` layouts и Engine через `gpu::with_session(Requirements, ...)`; scratch handles не переживают защищённую GPU операцию. CPU generation и Micro-A* вне mutex. На смене parent frame и принятых poses инвалидировать зависимые кеши; фиксированные contributions использовать только при доказанной неизменности.
- **Отбор.** Сохранить порядок dedupe, групповых legal filters и shortlist. Readback — scores/ID нужных победителей, не вся геометрия всех вариантов. Новые lower bounds/pruning допускаются только после проверки эквивалентности unpruned GPU пути; веса и неположительные terms нельзя считать неотрицательными без проверки.
- **Точность.** Hard decisions, shortlist и законченный детерминированный поиск должны сохранять CPU результат. Проверка scores с численным допуском сама по себе не разрешает смену победителя. Для опасных границ воспроизвести арифметику либо явно перевести неподдерживаемый вход на CPU; не прятать массовый CPU rescoring в production и не обновлять reference под расхождение.

## Выбор backend и восстановление

Доменный переключатель — `PCB_BOARD_BACKEND=cpu|cubecl|auto`; это новый API реализации Board Packager. Текущие `PCB_BOARD_PACKER_THREADS`, profiling и NAPI контракт сохраняются. `cpu` не инициализирует GPU; `cubecl` не обходит guards; `auto` включает GPU только на измеренно выгодных размерах с учётом cold/ready runtime. Порог block solver не переносить автоматически. Существующий общий запрет инициализации через `PCB_BLOCK_GPU_DISABLED=1` сохраняется по [API](GPU_INFRASTRUCTURE_API.md); не заводить второй runtime ради нового имени настройки.

Guard проверяет **весь вход текущего native board call** до поиска. Промежуточная поддержка простых плат допустима для разработки, но завершение требует реальной GPU работы на объявленном контрольном наборе, а не CPU fallback на всех сложных платах. Неподдержанный term не пропускается и не считается нулём.

При runtime failure отбросить все частичные GPU states, placements, rankings и кеши, повторить весь `solveBoardPacked` с исходным `BoardPackProblem` на CPU, без GPU retries в этом вызове. Ordinary и aligned — два отдельных native calls; отказ второго не требует заново решать первый или пересобирать готовые блоки. Другие штатные ошибки входа не маскировать как успешный fallback. Busy/unsupported не должны отключать общий runtime; реальные runtime failures обрабатываются общей инфраструктурой.

## Этапы

### 1. Baseline и матрица покрытия

- [x] Прочитать root/native/PCB `AGENTS.md`, общую GPU API, весь board scorer и callers; найти существующие helpers, tests и replay прежде, чем добавлять новые.
- [x] Сохранить addon/source/input hashes, CPU outputs и stage timings для нескольких разных плат: Telemetry, esp32c3, ESPower; тяжёлый PortableScope использовать при наличии capture. Нужны valid references и отдельные stress/repair cases. Исторический невалидный PortableScope годится для profiling, не как доказательство качества.
- [ ] Измерить generation, legality, rank terms, route shortlist, Beam, joint pairs, local improve, repair, native wall и полный board stage (ordinary + aligned + выбор). Заморозить block/refiner backends и параметры.
- [x] Записать term/support matrix, точные исходные правила order/dedupe/rounding и ожидаемые CPU leftovers. Выбрать guard и кандидатов на общий код по данным, не по сходству названий.

**Выход:** воспроизводимые references и понимание, какую долю полного времени действительно занимает переносимая работа.

### 2. GPU evaluator на сохранённых batches

- [x] Создать доменный Engine и compact representation; перенести все активные hard/soft terms контрольного набора, legality и полные ranks. Обрабатывать и single-move, и two-move/full-state repair batches.
- [x] Сравнить каждый кандидат с CPU вне performance runs: geometry, hard count/severity, term scores, aggregate rank, IDs и порядок. Проверить compound/layer/outline/region/overflow/locked случаи и численные границы.
- [x] Проверить GPU shortlist и любые pruning с неотсечённым вариантом, chunk-size invariance, empty/small/large batches и cache invalidation. Не включать GPU по умолчанию до этих проверок.

**Выход:** корректный массовый scorer с измеренными upload/dispatch/readback и памятью, без CPU полной оценки каждого production кандидата.

### 3. Полная интеграция поиска

- [x] Подключить общий доменный evaluator к Beam, legality filtering, joint pairs, local improve и repair, сохранив группировку и CPU route policy.
- [x] Проверить результаты после каждой стадии и весь native call, включая ordinary/aligned варианты. Отдельно измерить ускорение каждой стадии и остаточную CPU стоимость.
- [x] Настроить batch sizes и `auto` по полным вызовам (сохранённые cold/warm baseline и один финальный проход согласно уточнению пользователя). Маленькие задачи должны оставаться выгодными; не менять качество/search width ради скорости.

**Evidence 2026-10-01:** финальный addon 19,26 МБ; 20 GPU tests, 24 native/assembly integration tests, 64 GPU / 60 CPU-only Rust tests и typecheck прошли. Telemetry: 10,51 млн неизменённых оценок, 5 040 вместо 22 506 batches; CPU reference переиспользован. ESPower: все 53 компонента, `placementOk=true`, точные placements и SVG. Не повторять stress/median series ради закрытия чекбоксов; открытые пункты означают ограничения имеющегося evidence.

**Выход:** ускоряется полный board solver, а не один демонстрационный kernel; фактический backend и причины fallback видны в отчёте.

### 4. Recovery, совместная работа и приёмка

- [x] Проверить disabled/no-device/unsupported/unsafe, failure внутри score и после Beam/local improve/repair, включая второй aligned call. Сравнить полный CPU replay с самостоятельным CPU запуском того же исходного входа.
- [x] Проверить 1/2/4 допустимых native workers, смешанное использование block → board → block в одном процессе и конкуренцию процессов/lease. Одна инициализация, независимые layouts/Engine, без oversubscription и смешивания результатов.
- [ ] Выполнить native build/tests с GPU и CPU-only, `npm test -- pcb-board`, `npm test -- pcb-block`, `npm run typecheck` и дополнительные затронутые geometry/path tests. Проверить единый release addon, размер, imports и изолированный CPU/GPU запуск.
- [ ] Проверить точные native inputs одним проходом на версию, на той же машине без конкурирующей GPU нагрузки, с одинаковым CPU budget; переиспользовать сохранённые CPU references. Требование трёх тёплых повторов отменено пользователем 2026-10-01. Отдельно проверить representative full boards, `placementOk`, hard violations, inventory, locked poses, ориентации, итоговые scores и previews.
- [x] Записать результаты, ограничения поддержки, backend coverage, hashes и changelog. Ниже сохранены ограничения приёмки; не выдавать отсутствующие проверки за выполненные.

**Приёмка:** контрольные valid boards остаются valid, детерминированные результаты сохранены, все заявленные массовые стадии работают на GPU; показано ускорение полного тяжёлого native board solve в согласованном одиночном замере и отсутствует регрессия малых `auto` задач. Конкретный множитель заранее не обещается. Если итоговое время не улучшилось, этап остаётся незавершённым с измеренным объяснением.

## Инструменты и артефакты

Использовать [native capture/replay](../../../pcb-layout-debugging.md): `npm run debug:pcb-layout -- replay <native-board-capture> <repeats>`; полный capture — отдельная проверка интеграции. `scripts/experiment-board-replay.mjs` сейчас меняет **block** experiments: не использовать его без адаптации как чистый board GPU benchmark. Сохранять native cache выключенным и проверять, что вызов реально выполнен.

Новые evidence — отдельный каталог `debugging/board-gpu-<date>/`, выводы и команды — соседний `BOARD_PACKAGER_GPU_RESULTS.md`, созданный при реализации. Не перезаписывать старые captures/references. CPU/GPU candidate validation выключена во всех замерах скорости; суммы worker times не приравнивать к wall time.

Следующая задача — [Post-place / Refiner](POST_PLACE_GPU_ROADMAP.md). Не включать её kernels в этот этап ради более красивого общего ускорения платы.

## Передача следующему этапу, 2026-10-01

Пользователь поручил перейти к Post-place / Refiner после коммита этой работы. Открытые чекбоксы выше описывают более широкую исходную программу измерений: полное TS portfolio Telemetry, отдельный полный профиль всех мелких стадий и всю исходную регрессионную матрицу на последнем addon. Они не выполнены заново и не являются заявленными результатами. Основная реализация и выявленные недостатки закрыты; дальнейшие массовые повторения ради чекбоксов не требуются. Для следующего roadmap действуют те же один проход на тест/версию и переиспользование CPU references.
