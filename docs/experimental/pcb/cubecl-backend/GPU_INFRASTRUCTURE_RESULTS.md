# Выделение общей GPU-инфраструктуры

2026-09-30. Статус: реализация и приёмка завершены на Windows x86-64 / RTX 3060 Laptop. Область — общий runtime, scratch, типизированные ошибки и численные probes внутри прежнего addon. Block scoring/search, CPU F64 и публичные NAPI/TS контракты сохранены. API и владение памятью: [GPU_INFRASTRUCTURE_API.md](GPU_INFRASTRUCTURE_API.md).

## Исходная серия

Основной checkout, `feat/block-solver-cubecl`, исходный revision `6d1f9c8` (документация поверх реализации `8ff7564`). Сохранённый addon: 17 686 528 байт, SHA-256 `b0388a2a1bc3b281a687f31ad3913b1d448df0d3ad7f9f61efca605c82d07aaa`. Копия, manifest и исходное дерево файлов находятся в `debugging/gpu-infrastructure-2026-09-30/baseline/`. Исторические evidence не перезаписывались.

Первая CPU серия и прерванный GPU запуск выполнялись одновременно с Valheim при 100% загрузке GPU. Они сохранены как диагностика и исключены из приёмки скорости; причина записана в `baseline/concurrent-gpu-load.log`. После освобождения GPU повторены CPU и GPU серии: пакет `00079,00097,00100,00091`, полный native cycle со штатными deferred pairs, первый запуск плюс три тёплых повтора. В каждом запуске все outputs/checkpoints точно совпали с CPU reference.

| Исходный backend / workers | Первый пакет, с | Тёплая медиана, с | Инициализации | Максимальный scratch, байт |
|---|---:|---:|---:|---:|
| CPU / 1 | 136.202 | 135.545 | 0 | 0 |
| GPU / 1 | 19.914 | 16.194 | 1 | 1 383 944 |
| GPU / 2 | 19.734 | 15.840 | 1 | 1 383 944 |
| GPU / 4 | 19.184 | 15.393 | 1 | 1 383 944 |

GPU / 1: FPGA Beam 8.326 с, singles 5.495 с, полный native cycle 13.820 с. CPU / 1: FPGA полный cycle 132.028 с. Артефакты — `final-baseline-idle-{cpu,gpu}-w{1,2,4}/results.json`; сводка — `final-summary.json`. Замеры включают подробные прежние профильные логи; validation flags выключены, сравнение результата и запись файлов выполняются вне таймера. GPU baseline использует сохранённый addon и `sourceReport` исходной CPU серии; инструмент проверяет совпадение addon hash, прежде чем наследовать source evidence.

Маленькие/unsupported входы `00085,00082,00065` сохранены в `baseline-small-cpu` и `baseline-small-gpu`. На принудительном GPU сохраняется известное отличие равнозначных poses `00082`; scores/hard counts совпадают с CPU. `00065` остаётся unsupported и вычисляется CPU. Reference под миграцию не менялся.

## Подтверждённые проверки

- `cargo test --locked --manifest-path native/pcb-board-packer/Cargo.toml`: 63 passed, 2 ignored. Перенесённые tests проверяют halfway boundaries, большой диапазон, 2 000 001 последовательное и 100 000 случайных integer/grid значений. `rust-default.log`.
- Тот же вызов с `--no-default-features`: 60 passed, 1 ignored. `rust-cpu-only.log`; `cpu-only-dependencies.log` не содержит CubeCL/wgpu.
- Отдельный GPU test `compute::gpu::tests::shared_workspace_alternates_layouts_and_releases_on_failure`, `-- --ignored --exact --nocapture`: passed на реальной RTX 3060. Два layouts с локальным slot 0 и F64/U32 операциями: 0/1/129/4097/1/8193/129 элементов, разные размеры, независимые outputs, рост и reuse capacities. Проверены `ready()` без init, одна инициализация, отказ входа без отключения runtime, panic, scratch bytes 0 после failure и запрет повторной GPU попытки. `shared-workspace.log`.
- `extraction-integrity.json`: все scoring/pruning/frontier kernels сохранены; операции rounding и прежние boundary tests сохранены при переносе.
- Release `placement-bench` сборка и существующий `experiment-block-gpu-runtime.mjs`: normal F64 4097/1/8193/129 exact, одна инициализация, disabled import/rejection и panic containment passed. `runtime/results.json`, `runtime-build.log`, `runtime-smoke.log`.
- `npm run native:build`: passed. Production `.node` — 17 702 400 байт, SHA-256 `78716e1c96e9fdcbf4e22de7046dbad36830002ffdf6f4aeb8f916ce97eeb476`. Рост относительно baseline — 15 872 байта. `native-build.log`, `production-manifest.json`.
- Import table содержит только системные Windows DLL; активное дерево зависимостей не содержит OpenCL, CUDA/HIP или CPU LLVM/JIT. `production-imports.log`, `production-dependencies.log`. Изолированная папка содержит один `solver.node`; production bench API отсутствует.
- `npm test -- pcb-block`, `PCB_BLOCK_BACKEND=cpu`: 28 passed, 0 failed на production addon; `block-tests.log`. GPU paths проверяются отдельно native captures. `npm run typecheck`: passed; `typecheck.log`.

Все перечисленные локальные артефакты находятся под `debugging/gpu-infrastructure-2026-09-30/`.

## Validation и восстановление

`verify-candidates`: все 6 790 066 кандидатных оценок четырёх полных блоков сравнены с CPU, hard counts точны, scores проверены с прежним допуском `1e-8 + abs(CPU)*1e-12`; сравниваются порядок compact poses, геометрия/метаданные representative templates и frontier. Все outputs/checkpoints и deferred pair результаты точно совпали с CPU baseline. Реальный backend — CubeCL, fallback отсутствует. Эти диагностические CPU пересчёты выключены в production/performance runs.

`verify-pruning` отдельно сравнивает каждый GPU shortlist с пересчётом того же batch без pruning: порядок, ID, hard counts и биты score совпадают. Проверено 9 691 364 оценки, включая дополнительные проверки unpruned batches. `verify-frontier` отдельно сравнивает каждую scarcity с CPU; FPGA содержит четыре frames и 3 888 768 frontier checks. `verify-no-pruning` подтверждает точный полный результат пакета с выключенным pruning. Во всех четырёх режимах результаты точны и fallback отсутствует; `validation-summary.json` содержит backend/hash/counters.

`recovery/summary.json`: disabled GPU, скрытый Vulkan driver discovery, panic внутри восьмого GPU batch, отказ после Beam, после singles и внутри отдельного deferred-pairs call. Все шесть режимов возвращают полный исходный CPU результат; процессы остаются живы. Для pairs runner берёт исходный `pairSeed` из полного CPU reference, сохраняет hash реально воспроизводимого входа и сравнивает самостоятельный pairs output с reference. Частичные GPU poses/checkpoints не используются.

`guard-locked-gpu` и `guard-numeric-gpu`: сохранённые ранее synthetic inputs скопированы в отдельную папку рядом с неизменённым поддерживаемым `00097`. В одном процессе каждый проблемный native call повторяется CPU, а следующие поддерживаемые calls успешно используют GPU. Результат полного смешанного пакета точен, runtime создаётся один раз, доменный отказ его не отключает. Original captures и прежние synthetic evidence не редактировались.

`small-gpu`: `00085,00082,00065` точно совпадают с новым замером сохранённого исходного GPU addon, включая известные равнозначные poses `00082`. `small-auto` точно совпадает с исходным CPU результатом, не создаёт runtime и оставляет `00065` unsupported. Полные isolated cycles двух блоков с GPU и GPU disabled проходят с точным CPU результатом: `isolated-gpu`, `isolated-disabled`; папка поставки содержит один production `.node` без bench API.

`lease-after-failure/summary.json`: после runtime panic первый процесс вернул точный CPU результат и остался жив; второй получил lease, создал runtime один раз и вернул точный GPU результат. Это проверка освобождения session/scratch/lease, без обещания уничтожить глобальный CubeCL allocator/context.

`processes/summary.json`: конкурирующие 1/2/4 Node процесса выполняют по два полных блока. В каждой группе один владелец GPU с одной инициализацией, остальные CPU fallback; все результаты точны. Wall time групп — 5.499/6.074/9.543 с. Whole-device peaks — 2334/2328/2478 MiB, включая другие приложения; это не размер scratch или выделенная исключительно addon память. Общая функциональная сводка — `functional-summary.json`.

## Производительность и контроль разброса

Все серии ниже используют один и тот же пакет `00079,00097,00100,00091`, F64, штатные deferred pairs, первый запуск процесса и три тёплых повтора. Validation flags выключены; профиль включён прежним flag. Сборки и тесты во время замеров не запускались. Во всех повторах outputs/checkpoints точно совпадают с CPU reference. GPU серии действительно используют CubeCL без fallback; CPU серии не создают runtime. Артефакты — `final-baseline-idle-*`, `final-after-*`, `final-control-*`; отдельный `final-after-auto` содержит только два запуска.

| Backend / workers | Исходный: первый пакет, с | Новый: первый пакет, с | Исходный: тёплая медиана, с | Новый: тёплая медиана, с | Mutex wait исходный / новый, мс |
|---|---:|---:|---:|---:|---:|
| CPU / 1 | 136.202 | 139.135 | 135.545 | 140.759 | 0 / 0 |
| GPU / 1 | 19.914 | 20.483 | 16.194 | 16.810 | 0.421 / 0.767 |
| GPU / 2 | 19.734 | 20.177 | 15.840 | 16.421 | 9 340.746 / 9 938.383 |
| GPU / 4 | 19.184 | 21.328 | 15.393 | 16.203 | 28 426.143 / 33 976.899 |

Mutex wait — максимальное накопленное значение за все четыре запуска в процессе, суммарное по native threads; это не задержка одного вызова. При 2/4 workers GPU операции сериализованы общим mutex, а CPU генерация и диспетчеризация конкурируют за него. Для каждого GPU процесса число инициализаций равно 1, максимальный scratch равен прежним 1 383 944 байтам. Число batches/candidates для одинаковых workers сохранено; порядок получения mutex не фиксирован.

Полные стадии при GPU / 1 (тёплые медианы). В каждой ячейке: Beam / singles / pairs / полный native cycle, секунды. Для FPGA pairs не выполняются: остаток профильного времени между стадиями не является pair scoring.

| Блок | Исходный ранний | Новый основной | Исходный поздний контроль | Новый поздний контроль |
|---|---|---|---|---|
| 00079 | 0.320 / 0.013 / 0.218 / 0.550 | 0.326 / 0.015 / 0.241 / 0.585 | 0.343 / 0.014 / 0.235 / 0.588 | 0.358 / 0.015 / 0.231 / 0.629 |
| 00097 | 0.685 / 0.025 / 0.541 / 1.249 | 0.689 / 0.025 / 0.575 / 1.313 | 0.717 / 0.025 / 0.583 / 1.325 | 0.712 / 0.026 / 0.588 / 1.327 |
| 00100 | 0.325 / 0.013 / 0.213 / 0.549 | 0.333 / 0.014 / 0.230 / 0.578 | 0.344 / 0.014 / 0.233 / 0.587 | 0.335 / 0.014 / 0.232 / 0.581 |
| FPGA 00091 | 8.326 / 5.495 / — / 13.820 | 8.505 / 5.854 / — / 14.315 | 8.834 / 5.841 / — / 14.676 | 8.794 / 6.025 / — / 14.821 |

Медианы стадий считаются независимо и могут не складываться в медиану полного времени. CPU FPGA: ранний исходный полный cycle 132.028 с, новый 136.795 с, поздний исходный 134.927 с. Изменение CPU также наблюдается при сохранённом CPU коде.

Основная серия показывает рост тёплого времени CPU на 3.85%, GPU / 1 на 3.80%, GPU / 2 на 3.67%, GPU / 4 на 5.27% относительно раннего исходного замера. Поэтому исходный addon повторно запущен после новой серии; поздний контроль нового GPU также повторён. Игра уже закрыта, но условия ноутбука меняются: отсутствие игры не гарантирует одинаковую частоту, power state и фоновые нагрузки.

| Контроль / addon | Первый пакет, с | Тёплая медиана, с | Диапазон трёх тёплых запусков, с |
|---|---:|---:|---:|
| CPU / исходный поздний | 138.514 | 138.564 | 137.612–142.523 |
| CPU / новый основной | 139.135 | 140.759 | 140.518–142.628 |
| GPU / 1 / исходный поздний | 21.271 | 17.189 | 17.143–17.349 |
| GPU / 1 / новый поздний | 22.043 | 17.379 | 17.073–17.775 |

Исходный GPU сам замедлился на 6.15% между ранней и поздней сериями. Разница поздних GPU медиан составляет 1.11%, CPU нового против позднего исходного — 1.58%; диапазоны перекрываются. Эти данные не показывают устойчивого замедления, объяснимого только выделением инфраструктуры. Приёмка скорости — сохранение уровня исходного backend в наблюдаемом разбросе, без заявления статистически доказанной эквивалентности или нового ускорения. Алгоритмы, thresholds, reference и F64 точность ради времени не менялись.

`auto`: первый пакет 21.748 с, второй 17.443 с. Первые три небольших initial calls используют CPU, затем FPGA и deferred pairs — CubeCL; после появления ready runtime все семь calls второго пакета используют CubeCL. Одна инициализация, fallback отсутствует, результаты точны. Второе время — один тёплый контроль, не медиана трёх повторов.

`performance-verdict.json` содержит вычисленные медианы/диапазоны и контроль counters. `final-integrity.json` проверяет hashes текущих исходников, окончательного и сохранённого исходного addon, captures и всех выбранных performance reports. Сводка `final-summary.json` также содержит исключённые ранние запуски; их нельзя включать в сравнение скорости.

## Проверенная среда и ограничения

Windows x86-64, Ryzen 5 5600H, NVIDIA GeForce RTX 3060 Laptop GPU 6 GiB, driver 610.88; Rust/Cargo 1.98.1, Node 26.5.0, CubeCL 0.10.0 / wgpu 29.0.4. Другие OS/GPU и физическая карта без F64/U64 этой проверкой не охвачены. Проверка отсутствующих consumer capabilities выполняется на синтетических capabilities; это не аппаратная проверка другой карты.

Runtime остаётся одним на процесс и сериализует GPU операции общим mutex. `workspaceBytes` учитывает scratch, а не всю VRAM; CubeCL allocator/context может оставаться до завершения процесса. F32, board packing, post-place/refiner, islands и принятый ранее CPU USB техдолг не решаются этим выделением.

## Передача следующей задачи

Закрыт только [roadmap инфраструктуры](GPU_INFRASTRUCTURE_ROADMAP.md). Следующий отдельный roadmap — Board Packager: сначала сохранить валидные board inputs/CPU references и обновить профиль `solver.rs::score_positions` в search, local improve и repair. Затем определить массовые batches compound primitives, board/region bounds, hard constraints, relations, rank semantics и собственную границу полного CPU replay. Существующие controller и маршрутная проверка shortlist сохраняются до отдельного решения. Новый scorer получает свой scratch layout и resident Engine через описанный общий API; копия runtime не нужна.

Post-place/refiner потребует отдельного профиля общего `post_place.rs::score` и массовых оценок `post_place_refine.rs`, с разграничением одинаковой работы и качества за одинаковый timeout. Его реализация и F32 миграция здесь не начинались. Исторический Telemetry профиль полезен для выбора точки чтения, но не заменяет новый baseline после ускорения block solver.
