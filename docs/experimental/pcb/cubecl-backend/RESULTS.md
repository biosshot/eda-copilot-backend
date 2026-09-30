# CubeCL block solver: реализация и проверка

Дата: 2026-09-30. Ветка `feat/block-solver-cubecl`. Область: только native block solver. Board packing и общий post-placement refinement не переносились.

## Поведение

- Массовые оценки Beam, singles и reinsert-pairs выполняются в Rust/CubeCL на GPU F64. CPU сохраняет прежнюю F64 арифметику, генерирует compact poses и выполняет разрешённую маршрутную доводку короткого shortlist. Небольшие финальные наборы pair/swap вариантов остаются CPU-задачами.
- Cheap score распределяет независимых кандидатов по GPU lanes; full score запускает workgroup из 128 lanes на кандидата, параллельно проверяет pads и сводит integer counts. Одновременно обрабатывается множество кандидатов/workgroups. Последовательный MST внутри кандидата не превращает весь батч в работу одного GPU ядра.
- Выбор следующего primitive также использует GPU: core-relative distance/nearby frame кешируется по положениям и ориентациям cores, проверки относительно изменяющейся расстановки исполняются батчем. CPU получает только целочисленные счётчики для прежней scalar frontier formula.
- Один ленивый Vulkan/CubeCL runtime на процесс. Native workers используют общий mutex и workspace; статические данные и кеши отдельных блоков принадлежат своим Engine. Между Node process действует освобождаемая ОС lease: занятый GPU означает CPU fallback.
- По умолчанию `PCB_BLOCK_BACKEND=auto`: меньше 6 primitives — CPU; отдельные небольшие блоки не инициализируют GPU. Инициализация разрешена от 10 primitives или 240 connection points при минимуме 6 primitives; после неё подходящие блоки от 6 primitives переиспользуют runtime. Эти границы не меняют ширину поиска или количество стадий.
- `PCB_BLOCK_BACKEND=cpu` выбирает исходный CPU solver. `PCB_BLOCK_BACKEND=cubecl` позволяет измерять GPU на поддерживаемых блоках от 3 primitives. `PCB_BLOCK_GPU_DISABLED=1` запрещает инициализацию GPU.
- Неподдерживаемая геометрия/функция, отсутствие GPU F64/U64, ошибка инициализации, численно небезопасный вход или runtime failure приводят к повтору всего текущего native block call на CPU с исходным входом. Частичные GPU placements/checkpoints не используются. Deferred pairs — отдельный штатный вызов со своим исходным pairSeed.

Граница поддержки унаследована от проверенного OpenCL scorer: до 20 одиночных primitives/components, четыре ортогональные ориентации, component collision modes, совместимые local-access/pad-crossing/stable-net flags. Locked/world/bounds/obstacles/targets/path-port и другие неподдержанные terms переводят весь блок на CPU. Power-yield допускается только там, где guard доказывает его нулевой вклад. Полная таблица — [SCORE_MAP.md](SCORE_MAP.md); код guard — `cubecl.rs::supported`. Это не универсальная GPU-поддержка любых входных blocks.

## Точность

Vulkan F64 деление на 1000 вернуло `-0.5650000000000001` вместо CPU `-0.565`; на границе clearance это меняло штраф на 10 миллионов. Дополнительно обнаружена FMA-контракция в округлении ровно половины шага. Допуск зазора и CPU scorer не менялись.

GPU `rp` теперь воспроизводит округление CPU: получает биты отдельно округленного произведения, а деление целочисленной тысячной доли заменяет F64-приближением с точной целочисленной коррекцией остатка. Обычная Rust-версия того же `#[cube]` helper проверена на 2 000 001 последовательном и 100 000 случайных целых значениях, плюс соседних значениях около половины шага и большом диапазоне. Runtime один раз проверяет реальный GPU kernel на граничных значениях до запуска поиска. Экспериментальные FloatControls2/fast_math flags удалены после native crash; рабочий путь их не использует.

На основной тройке FPGA + 6-cap + 8-cap:

- Exhaustive validation: 2 641 322 полных кандидатных оценки, точные hard counts, score в пределах `1e-8 + abs(CPU)*1e-12`. Проверены порядок poses и геометрия/метаданные representative templates каждого батча.
- Отдельно сравнивалась каждая frontier-scarcity с исходной CPU функцией, включая меняющуюся legality и четыре core frames.
- При отдельном запуске каждый GPU shortlist с pruning сравнивался с пересчётом того же батча без pruning: порядок, ID, hard counts и биты score совпали.
- Итоговые placements, scores, checkpoints и deferred pair outputs основной тройки точно совпали с CPU. FPGA final score — `5528.140155316541`, hardCount — `0`.

Дополнительный 4-cap block (`00082`) при принудительном GPU режиме выбрал другие равнозначные ориентации/позы; scores и hard counts совпали на checkpoints и в итоге. Это отличие не скрыто обновлением reference. Такой маленький блок по умолчанию остаётся на CPU. Дополнительный 12-component regulator (`00065`) оказался вне capability guard и был полностью пересчитан CPU; его время не считается GPU-ускорением.

Артефакты находятся под `debugging/cubecl-block-migration-2026-09-30/`: `verify-final`, `verify-pruning`, `verify-frontier`, `verify-cache-extended`, `cpu-small-threshold`, `gpu-small-threshold`. В `results.json` сохранены SHA-256 addon, исходников и inputs, полные outputs и реальный backend. Валидационные запуски с CPU пересчётом не включаются в замеры скорости.

## Восстановление и упаковка

- `recovery-no-device`: GPU отключён и Vulkan driver discovery направлен на отсутствующий manifest. Все native calls завершились точным CPU результатом, процесс остался жив. Способ скрытия драйвера описан в [официальной документации Vulkan Loader](https://github.com/KhronosGroup/Vulkan-Loader/blob/main/docs/LoaderDriverInterface.md#overriding-the-default-driver-discovery).
- `recovery-mid`: panic внутри восьмого GPU батча, отказ после Beam и после singles. Во всех случаях возвращён полный CPU результат с исходного входа. Никакого продолжения с частичной GPU расстановки.
- `guard-summary.json`: отдельные синтетические копии capture с locked primitive и значением `1e13` в clearance matrix. Обе дали полный CPU fallback и точное совпадение с самостоятельным CPU вызовом. Оригинальные captures не изменены. Реальная карта без F64/U64 отдельно не была доступна; отсутствие совместимого устройства проверялось скрытием Vulkan driver discovery, а наличие возможностей проверяется перед kernel.
- Production addon: **17 686 528 байт** (17.687 MB; 16.867 MiB), SHA-256 `b0388a2a1bc3b281a687f31ad3913b1d448df0d3ad7f9f61efca605c82d07aaa`.
- `isolated-production/solver.node` импортируется из папки, где находится только этот файл; production API не экспортирует runtime-bench helper. Полные циклы двух блоков с GPU и с GPU disabled прошли с точным CPU результатом (`isolated-gpu`, `isolated-disabled`).
- Import table содержит только системные Windows DLL. Active Cargo dependency tree не включает OpenCL, CUDA/HIP или CPU LLVM/JIT backend. Статический CRT включён в `.node`; GPU driver остаётся системной зависимостью.
- Проверки: Rust default **62 passed, 1 ignored**; Rust CPU-only **60 passed, 1 ignored**; focused TS block **28 passed**; TypeScript typecheck passed.

## Среда и финальные измерения

Windows x86-64; AMD Ryzen 5 5600H (6 cores / 12 threads); NVIDIA GeForce RTX 3060 Laptop GPU (6 GiB), driver 610.88; Rust 1.98.1; Node 26.5.0; CubeCL 0.10.0 / wgpu 29.0.4. Другие GPU, драйверы и платформы этой проверкой не охвачены.

Финальные измерения выполняются отдельно от validation, сборок и тестов, на одной production сборке. Пакет включает `00079`, `00097`, `00100`, `00091`: четыре полных block cycles, с deferred pairs для меньших блоков; у 19-primitive FPGA pairs штатно пропускаются. Первый запуск отделяется от трёх тёплых. Проверки результата и запись JSON находятся вне измеряемого участка.

### Полный цикл и стадии, один native worker

Медианы трёх тёплых повторов, секунды. Каждый столбец агрегируется отдельно, поэтому сумма медиан стадий может немного отличаться от медианы полного времени.

| Блок / стадия | CPU F64 | CubeCL F64 | Ускорение |
|---|---:|---:|---:|
| FPGA Beam | 38.682 | 8.367 | 4.62× |
| FPGA singles | 88.463 | 5.394 | 16.40× |
| FPGA полный native cycle | 127.146 | 13.748 | **9.25×** |
| 00079, 6 capacitors, полный cycle с pairs | 0.752 | 0.568 | 1.32× |
| 00097, 8 capacitors, полный cycle с pairs | 1.925 | 1.271 | 1.51× |
| 00100, 6 capacitors, полный cycle с pairs | 0.743 | 0.556 | 1.34× |
| **Пакет из четырёх полных blocks, API wall time** | **130.600** | **16.159** | **8.08×** |

Pairs для 00079/00097/00100: CPU 0.342/0.954/0.335 с; GPU 0.226/0.556/0.218 с (ускорение 1.52/1.71/1.54×). Это включает CPU обработку маленьких финальных pair/swap наборов; массовые reinsert оценки исполняет GPU. Singles для этих блоков: CPU 0.058/0.140/0.059 с; GPU 0.013/0.025/0.013 с. Beam: CPU 0.350/0.833/0.350 с; GPU 0.328/0.691/0.325 с. Маленькие Beam получают небольшое преимущество, поэтому отдельный небольшой блок не должен оплачивать GPU startup.

Первый запуск пакета: CPU **133.607 с**, GPU **20.077 с** (6.65×). Его первый FPGA вызов: Beam 40.536/8.818 с, singles 89.394/5.386 с, полный native cycle 129.931/14.205 с (CPU/GPU). FPGA стоит последним в пакете и использует runtime, уже созданный первым блоком. Время пакета включает startup, упаковку NAPI и все стадии; внутренние времена блока исключают создание Context/runtime перед поиском. «Первый» означает первый в новом процессе, а не очищенный системный shader cache.

Тёплые полные времена пакета: CPU 130.178, 132.354, 130.600 с; GPU 16.159, 16.185, 16.141 с. Все четыре результата и checkpoints в каждом GPU повторе **точно совпали с CPU**. CPU повторы также идентичны. Ни reference, ни captured inputs для этого не изменялись.

Артефакты: `final-cpu-w1/results.json`, `final-gpu-w1/results.json`, общий `final-summary.json` под каталогом эксперимента. Summary воспроизводится `node scripts/report-block-cubecl.mjs`.

### Исторический Beam и восстановленная организация

Исторические OpenCL 8.318 с и нынешние CubeCL 8.367 с различаются на **0.6%**; старое число получено в отдельной исторической серии, поэтому это не доказательство превосходства одного runtime над другим. Быстрый путь восстановлен до прежнего диапазона 8–10 с с GPU F64. Полный block теперь дополнительно ускоряет singles.

На FPGA Beam: 589 score-батчей / 1 282 646 кандидатов; singles добавляет 114 батчей / 463 122 кандидата. Frontier: 208 батчей / 3 888 768 проверок, четыре core frames. Массовые оценки, pruning и bounded shortlist выполняются GPU; CPU не пересчитывает кандидатов в обычном режиме. Полный cycle — 703 score-батча / 1 745 768 score-кандидатов плюс frontier.

При переносе были устранены три измеренных причины отставания: повторная сборка движущихся pad boxes (теперь shared cache на кандидата), дорогая реализация точного деления координаты (теперь F64 approximation + integer correction), пересчёт CPU frontier_scarcity (6.788 с на 1908 вызовов; восстановлен core-frame cache и массовые проверки перенесены на GPU). Промежуточные измерения служили диагностике; итоговая таблица использует только замороженную production сборку.

### Конкуренция

Четыре различных полных блока через native batch API; GPU — первый + три тёплых повтора, CPU при 2/4 workers — по одному контрольному полному запуску.

| Native workers | GPU первый пакет, с | GPU тёплая медиана, с | Блоков/с, тёплые | Суммарное ожидание mutex за тёплый пакет, с |
|---:|---:|---:|---:|---:|
| 1 | 20.077 | 16.159 | 0.248 | <0.001 |
| 2 | 19.537 | 15.721 | 0.254 | 1.434 |
| 4 | 18.910 | 15.155 | 0.264 | 4.310 |

Ожидание суммируется по потокам, поэтому не прибавляется к wall time. Во всех сериях ровно одна GPU инициализация на процесс, 1 383 944 байта (1.320 MiB) максимального общего workspace и точное совпадение всех outputs/checkpoints с CPU. Это размер переиспользуемых рабочих buffers, **не вся VRAM**: resident данные блоков, pipelines и runtime allocator учитываются отдельно. CPU контроль с 2/4 workers: 128.964/128.976 с, точный результат. Артефакты: `final-cubecl-w2`, `final-cubecl-w4`, `final-cpu-w2`, `final-cpu-w4`.

Выбран один workspace на общем runtime. На данном пакете 4 workers дают 6.6% роста throughput относительно одного; они скрывают часть CPU подготовки, но увеличивают latency отдельных небольших блоков из-за mutex. Данных в пользу дополнительных GPU contexts нет. Пул workspace не объявляется бесполезным или оптимально настроенным: отдельного сравнения с реализацией пула не проводилось, первый вариант сохраняет простую проверенную изоляцию и ограниченный workspace.

Отдельный тест **1/2/4 Node solver processes**, по два полных блока 00079/00097 на каждый новый процесс, принудительный GPU запрос. Это проверка конкуренции и холодного запуска; CPU fallback участников включён в wall time.

| Процессов | Фактическое исполнение | Общее время, с | Полных блоков/с | VRAM до / пик / после, MiB |
|---:|---|---:|---:|---:|
| 1 | 1 GPU | 5.559 | 0.360 | 1282 / 1990 / 1271 |
| 2 | 1 GPU + 1 CPU fallback | 6.037 | 0.663 | 1275 / 2047 / 1341 |
| 4 | 1 GPU + 3 CPU fallback | 8.001 | 1.000 | 1338 / 2092 / 1353 |

GPU API latency: 5.187/5.641/7.516 с; CPU fallback latency при 2 процессах 3.416 с, при 4 — 5.047–5.087 с. Рост throughput здесь достигается совместной работой CPU/GPU, а не одновременными GPU contexts. Все результаты точно совпали с CPU; у проигравших lease процессов 0 GPU инициализаций, причина fallback — `GPU owned by another process`. VRAM измерялась `nvidia-smi` каждые 500 мс на всём устройстве, включая остальные приложения; это наблюдаемый пик, не точный счётчик аллокаций solver. Артефакт: `processes-final/summary.json`.

`lease-after-failure`: первый процесс получил panic внутри GPU батча, вернул точный CPU результат и **остался жив**. Второй процесс после этого успешно получил lease и выполнил полный GPU цикл с точным результатом. Session/workspace handles и lease освобождаются при runtime failure; глобальный клиент/allocator CubeCL может сохранять контекст драйвера до конца процесса. Немедленное уничтожение всего GPU context после ошибки не обещается.

### Автоматический controller и воспроизведение

`final-auto`: в новом процессе три небольших блока сначала считают CPU; FPGA создаёт GPU runtime, после чего deferred pairs используют GPU. Во втором полном пакете все семь native calls используют GPU. Оба полных результата точно совпали с CPU. Первый пакет 21.133 с, единственный дополнительный тёплый контроль 16.844 с; это проверка controller, не новая медиана трёх повторов. Принудительный запуск маленьких blocks на GPU дороже в первом процессе из-за initialization/compilation, поэтому default threshold сохраняется.

Основная серия из корня backend:

```powershell
node scripts/experiment-block-cubecl.mjs backend=cpu blocks=00079,00097,00100,00091 runs=4 workers=1 out=debugging/cubecl-block-migration-2026-09-30/final-cpu-w1
node scripts/experiment-block-cubecl.mjs backend=cubecl blocks=00079,00097,00100,00091 runs=4 workers=1 reference=debugging/cubecl-block-migration-2026-09-30/final-cpu-w1/results.json out=debugging/cubecl-block-migration-2026-09-30/final-gpu-w1
node scripts/report-block-cubecl.mjs
```

Для native concurrency меняются `workers=2`/`4` и выходной каталог. Отдельные инструменты: `experiment-block-gpu-recovery.mjs`, `experiment-block-gpu-processes.mjs`, `experiment-block-gpu-lease.mjs`. Exhaustive validation включается через `PCB_BLOCK_GPU_VERIFY=all`, pruning через `PCB_BLOCK_GPU_VERIFY_PRUNE=1`; эти флаги не используются для performance. Нативные исходники и addon после финальной сборки не менялись; `final-integrity.json` подтверждает соответствие текущих source/addon hashes всем финальным сериям.

Будущий перевод всех CPU/GPU floating вычислений на F32 остаётся отдельной задачей без срока. Коэффициент 1.6 не использовался для расчёта измеренного ускорения.
