# Независимая проверка analysis worker

Дата: 2026-10-21. Проверены изменения после `77d7ba1` по заданию `next-task-analysis-runtime.md`.

## Решение

Этап принимается. Блокирующих замечаний в проверенной области не обнаружено. Правки реализации в ходе ревью не потребовались.

Runtime использует ограниченную FIFO и отдельный однопроцессный workerpool для каждого слота. Это небольшая специфичная организация исполнения: она позволяет остановить занятый процесс через публичный API workerpool и дождаться выхода, не затронув другой слот. Общая compute-runtime библиотека не появилась.

Проверены монотонный deadline от принятия задания, освобождение места в очереди, отмена синхронной работы, отсутствие повторного запуска аварийной задачи, однократное завершение Promise, обработка progress callbacks, dispose и повторное создание runtime. Слот заменяется только после подтверждённой остановки. При ошибке остановки он остаётся занят; это ограничение описано явно.

Вызов addon находится в дочернем процессе. Исходный Rust-отчёт не переинтерпретируется: `imported: false` остаётся результатом импорта, инфраструктурные сбои имеют отдельные коды. `disposeBackend` пытается завершить оба пула даже при отказе placement shutdown.

## Независимые проверки

| Команда / проверка | Результат |
| --- | --- |
| `npm run typecheck` | PASS |
| `npm run build` | PASS |
| `node --import tsx --test --test-concurrency=1 tests/pcb-analysis-runtime.test.ts tests/public-api.test.ts tests/pcb-board-packer-native.test.ts` | 35 passed, 0 skipped; backend selectors — CPU |
| `node --test scripts/test-pcb-analyze.test.mjs` | 4 passed; hashes/inventory сохранённых входов проверены |
| `npm run test:pcb-analyze -- --all --name import` | 8 completed, 0 failed |
| Полные `report.json` именно этих восьми запусков против прямого addon | Все совпали, включая diagnostics и source IDs |
| `npm run test:package` после свежей сборки | PASS; analysis и placement workers установленного пакета, обычный режим и два режима недоступности GPU |
| `git diff --check` | PASS |

Логи независимой проверки: игнорируемые `.artifacts/review-runtime-*.log`. Пути восьми новых запусков находятся в `review-runtime-boards.log` (запуски 2026-10-21T16:30Z). Изолированный package consumer: `C:/Users/kiril/AppData/Local/Temp/eda-backend-package-D7XVcb`.

Rust, native binding, исходные платы и численная политика не менялись; повторная native-сборка и полная серия Rust/placement benchmarks не требовались. Реальный GPU, Linux/macOS, Node 20/24 и полный `npm test` локально не проверялись. Жёсткий лимит Rust RSS и общий CPU-бюджет с placement не реализованы и не заявляются.

Следующее задание: [физическая геометрия меди и связность](next-task-copper-geometry.md). Worker-этап сам по себе не подтверждает достоверность экспорта или готовность физических solver.
