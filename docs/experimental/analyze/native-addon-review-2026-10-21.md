# Независимая проверка общего Node addon

Дата: 2026-10-21. Область: изменения после `17f9bc3` — библиотечная граница packer/analyze и единый N-API host. Проверка не является валидацией будущих физических анализаторов.

## Вывод

Блок принимается: блокирующих замечаний в проверенной области не обнаружено. `native/node-addon` регистрирует старые placement bindings и новый `importAnalysisBoard`; packer и analyze остаются обычными Rust-библиотеками. Математика placement и семантика импортёра не изменены.

Loader и устанавливаемый binary перенесены в `native/node-addon`; старого совместимого loader больше нет. Это уточнение относительно исходного задания отражено в отчёте реализации. Проверены потребители в исходниках, скриптах, CI и package checks. Внешний код, обращавшийся прямо к старому внутреннему файловому пути, потребует обновления; публичный API пакета сохранён.

## Проверенная совместимость

- Сохранились 42 прежних runtime exports и version/numeric contracts; добавлен один binding импорта. Ошибки библиотеки преобразуются в прежние N-API статусы, без изменения сообщения.
- CPU replay и сохранённый baseline совпали по детерминированным результатам, quality/error поведению и смешанному порядку вызовов. Время отдельно не трактуется как доказательство ускорения.
- На восьми платах полный Node-отчёт совпал с standalone Rust API. Проверены ошибочный JSON, неизвестные поля/слои и отсутствие доступной заливки.
- F64 subnormal-геометрия импортёра сохраняется при чередовании с packer F32/FTZ-вызовами. Импорт не требует GPU initialization.
- Host сохраняет feature forwarding, effective release profile, vendor patch и версии прежних registry-зависимостей. Declarations принадлежат библиотекам и агрегируются host.

## Независимо выполненные проверки

| Проверка | Результат |
| --- | --- |
| `npm run native:build` | Успешно, Windows x64 MSVC; использован существующий Cargo target cache packer |
| `cargo test --locked --manifest-path native/pcb-board-packer/Cargo.toml` | 103 passed, 4 ignored, CPU backend selectors |
| `cargo test --locked --manifest-path native/analyze/Cargo.toml` | 13 passed |
| `node --import tsx --test tests/pcb-native-addon.test.ts` | 5 passed, без skip |
| Focused TS: native packer, score authority, post-place, solve cache, condition semantics, geometry tolerance, public API | 61 passed, 7 skipped |
| `npm run test:recovery` | 11 passed, 12 skipped — проверки реального GPU не выполнялись |
| `npm run typecheck` | Успешно |
| `npm run build` | Успешно |
| `npm run test:package` после свежей сборки | Успешно; installed-package smoke, типы, восемь импортов, прежние workers и CPU recovery |
| Host `cargo check --locked --no-default-features` | Успешно |
| Host `cargo check --locked --features placement-bench` | Успешно |
| Сохранённый `debugging/native-addon/compare.mjs` | Совпадение baseline и нового addon |

Host checks используют `--manifest-path native/node-addon/Cargo.toml`. Локальные логи проверки находятся в игнорируемой `.artifacts/review-*.log`; baseline/comparison — в игнорируемом `debugging/native-addon`. Сборка и package smoke повторены после потери видимого вывода первоначальной проверки, чтобы подтвердить завершение и проверить свежий dist.

SHA-256 проверенного native binary: `db16f4da4391a0d68e67119a0a451c8ba159844d7e1dbcd34fbc4df6e4c00096`.

Linux/macOS, матрица Node 20/24 и реальный GPU в этом локальном ревью не проверены. Соответствующие CI/hardware проверки остаются необходимыми; CPU fallback и compile-only feature checks их не заменяют. Существующие Rust warnings не выдаются за новые ошибки или за устранённые предупреждения.

Следующее задание: [исполнение анализа в отдельном процессе](next-task-analysis-runtime.md). Оно объединяет runtime, жизненный цикл задач, тесты и существующий launcher; физические solver в него не входят.
