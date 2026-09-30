# Перенос block scorer: карта и последовательность

2026-09-30. Первый этап реализации: прочитать архивные алгоритмы, перечислить все terms, определить границы батчей и проверки до запуска новых kernels. Пользователь расширил проверку с одного FPGA до нескольких блоков и требует полный цикл сборки. Плата и общий post-placement refinement остаются вне задачи.

## Источники, прочитанные перед переносом

- `experiment/cubecl-gpu-benchmark:.../src/block_solver/opencl.rs`: `supported`, `Engine`, подготовка static/frame данных, `ranked`, `materialize`.
- Та же ветка, `.../src/block_solver/opencl_candidates.rs`: compact generator, порядок и dedupe proposals.
- Та же ветка, `.../opencl-experiment/score.cl`: frame pads/hull/MST/hits, cheap/full kernels, reuse неизменных segments.
- Та же ветка, `.../opencl-experiment/select.cl`: параллельный bounded rank, seeds, pruning, compact shortlist. Последовательные альтернативы из эксперимента не переносить как основной путь.
- `archive/cubecl-placement-partial-20260930:.../src/compute/gpu.rs`, `math.rs`, `Cargo.toml`: CubeCL/wgpu запуск, F64 capability probe, общий process runtime, освобождаемая ОС GPU lease и panic containment. Не переносить частичный scorer, board или postrefine.

Полные пути и blob IDs находятся в ROADMAP. Исходный CPU: `native/pcb-board-packer/src/block_solver.rs`, база `dec3b5d`.

## Карта вычислений

| Правило CPU | GPU перенос / граница поддержки | Проверка |
|---|---|---|
| bbox area/perimeter, convex hull area/perimeter | cheap kernel; fixed hull один раз на frame, движущийся bbox на кандидата | одинаковые poses, terms; вырожденные hull |
| aspect и smooth aspect | cheap kernel с исходными весами | обе compactness политики/flags |
| primitive overlap penalty | cheap kernel, все конфликтующие пары и исходный clearance | касания и глубины около нуля, отдельно от hard count |
| component hard geometry | cheap kernel с integer count, layers и pair clearances | точное сравнение hard count |
| bounds/obstacles/world | архив их исключает; до переноса закрыть явным capability guard | unsupported input целиком CPU |
| dense IC access | cheap kernel, исходные роли, pin counts, halos и веса | Beam и singles; dense flag зависит от search width |
| power yield | архив доказывает ноль только для своего класса: power-only либо non-power primitive с >240 endpoints | другие случаи guard до отдельного GPU term; не объявлять ноль универсальным |
| scoped relations, distance limits, side preference, offsets | cheap kernel и compact endpoint refs | внутренний/отсутствующий endpoint, веса, rounding |
| external exposure | cheap kernel; local access и четыре corridors | незавершённая внутренняя связь не external |
| port facing | архив исключает соответствующие relation kinds | guard до реализации term |
| signal/ground spread | cheap kernel, net refs в исходном порядке | stable weight, ground cutoff, small blocks |
| target width/height | архив исключает targets | guard до реализации term |
| long local nets | ноль только при доказанном отсутствии подходящих cross-primitive 2-endpoint nets | flag/ignored nets/ground; иначе guard |
| direct pad crossings | full kernel: ordered MST, fixed segment hits/cache, параллельные pad checks и integer reduction | каждый кандидат offline, metadata layers/owners, cache invalidation |
| signal path topology | архив исключает path ports/relations | guard до реализации term |
| frontier scarcity / выбор следующего primitive | сохранённый CPU алгоритм и архивный core-frame cache; расстояния/проверки poses и legal counts переносятся на GPU, CPU получает только несколько счётчиков для выбора frontier | отдельно сравнить каждую scarcity с CPU; cache keys включают core poses; legality относительно остальных placed не кешировать |
| micro route correction | разрешённый CPU малый shortlist, исходный lazy rank | route correction и конечный выбор |
| lower bound/pruning | GPU: cheap → seed ranks → full seeds → safe pruning → full survivors → final ranks | pruning on/off одной точности + CPU reference |

Ни один активный неподдержанный term не отправляется в массовый CPU хвост. Guard означает CPU для всего блока и явную диагностику. Это начальная граница безопасного переноса, а не утверждение о завершении поддержки всех блоков.

## Стадии и границы

- Beam: compact poses, массовая оценка; глобальный shortlist 16 либо diversity 64 с четырьмя кандидатами на bucket, затем прежний CPU lazy route shortlist. Ordinals сохраняются.
- Singles: кандидаты заменяют элемент на его исходном индексе в current, а не добавляются в конец. Это сохраняет порядок net refs, MST ties и суммирования. Shortlist — глобальные 16 без Beam diversity; исходная route/improvement логика остаётся. При принятых перемещениях обновлять frame и инвалидировать зависимые caches. Compact pose должен учитывать исходную позицию/ориентацию шаблона: относительный move нельзя принять за абсолютный template offset.
- Pairs: reinsert использует тот же ranked scorer для двух последовательных вставок. Swap/финальные варианты требуют batched scores двух перемещений; малые наборы допустимо направлять на CPU до GPU подготовки. Не менять порядок, top-4, improvement threshold или запрет pairs при >12 primitives. FPGA из 19 primitives не покрывает эту стадию.
- Checkpoints/final output: только выбранные placements; штатная CPU реконструкция и оценка checkpoints не являются массовой оценкой всех кандидатов.

## Последовательность и доказательства

1. Ранний release `.node`: общий runtime, F64 capability, параллельный kernel/readback, startup без GPU, отказ/panic и размер/imports. Это runtime smoke, не ускорение scorer.
2. Compact generator + static/frame representation: проверить точное совпадение порядка/materialized poses для Beam и singles с CPU. CPU reference не обновлять под перенос.
3. Cheap/full score и fixed caches по карте; exhaustive offline сравнение на одинаковых candidates. Затем bounded ranking/pruning on/off.
4. Подключать Beam, singles, применимые pairs по очереди; после каждого измерить его вклад и полный цикл, не менять алгоритм для красивого времени.
5. Несколько неизменённых captured native blocks: FPGA, дополнительный тяжёлый блок и блок с реально выполняемыми pairs. Для каждого самостоятельный CPU reference. Unsupported/fallback явно выделять; CPU fallback не является GPU измерением.
6. Первый вызов и минимум три тёплых запуска на режим одной release сборки; CPU/GPU validation отдельно от таймера. Измерить отдельные блоки, их общий последовательный цикл, затем batch API с workers 1/2/4. Отдельно процессы 1/2/4. Без параллельных сборок/тестов во время замеров.

Статус: массовый F64 scorer, compact generator, resident frames, pruning/ranking и frontier реализованы и проверены. Exhaustive validation основной тройки проверила 2 641 322 оценки; отдельная проверка pruning on/off дала одинаковые shortlists. Четыре полных блока в финальной серии точно совпали с CPU. FPGA Beam 8.367 с, singles 5.394 с, полный cycle 13.748 с; подробные данные и область поддержки — [RESULTS.md](RESULTS.md). Артефакты сохранены в `debugging/cubecl-block-migration-2026-09-30/` с source/addon/input hashes.

Дополнение 2026-09-30: профиль выявил 6.788 с CPU frontier_scarcity на FPGA (1908 вызовов). Архив c0aaafb кешировал core-relative frame через FrontierScarcityFrame, но текущая база dec3b5d его пересчитывает. Новый gpu_frontier.rs переиспользует это разделение: GPU distance/core-legality и nearby mask один раз на core frame, затем batched GPU legality относительно изменяющегося placed; CPU получает только integer counts для исходной scalar frontier formula. Все scalar результаты сравнены с CPU и совпали; на FPGA четыре frame, 208 батчей и 3 888 768 проверок. Сравнение включено только в отдельную validation-серию.
