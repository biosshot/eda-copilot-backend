# Проверка process runtime анализа

2026-10-21. Реализация задания `next-task-analysis-runtime.md` подготовлена к
приёмке поверх `77d7ba1`. Без коммита и перехода к геометрии/физике.
Локальная платформа: Windows x64, Node v26.5.0.

```text
parent runAnalysisProblem → ограниченная FIFO → process worker → общий .node
                                                            → Rust import_json
                         ← исходный AnalysisImportReport ←───────────┘
```

Границы и настройки подробно описаны в [README runtime](../../../src/analyze/README.md).
Выполняется только структурный импорт. `imported: true` не означает готовность
геометрии или достоверность физического расчёта.

## Изменённые файлы

- `src/analyze/types.ts`: внутренняя задача/прогресс, типизированные ошибки;
  тип отчёта импортируется из деклараций analyze.
- `src/analyze/analysis-pool.ts`: ограниченная очередь и слоты process workers,
  deadline, отмена, остановка и очистка.
- `src/analyze/run-analysis.ts`: ленивое поколение runtime и завершение.
- `src/analyze/run-analysis.worker.ts`: проверка запроса, загрузка общего addon,
  реальное событие `started` и единственный native-вызов.
- `src/analyze/README.md`: семантика и ограничения исполнения.
- `src/pcb.ts`: `disposeBackend` пытается завершить оба пула через allSettled.
- `tsup.config.ts`: analysis worker и внутренний runtime entry point.
- `scripts/check-package.mjs`: установленный process worker, восемь плат,
  diagnostics, перезапуск и прежний placement worker.
- `tests/pcb-analysis-runtime.test.ts`: 19 целевых проверок.
- `tests/fixtures/analysis-worker.cjs`: управляемая синхронная работа/авария,
  только для тестов; ready/release-файлы и watchdog.
- `tests/fixtures/analysis-addon-pid.cjs`: test-only наблюдение PID загрузки addon.
- `tests/helpers/pcb-analysis-import.ts`: общий сценарий чтения/сохранения отчёта
  и освобождения runtime в finally.
- `tests/pcb-analyze/{BandAmpV5,ESPower,NanoScope,NanoSpec,STM32G0_CAN_RS485,TPS63060DSCR,ThunderF722,USB_PD_Trigger_5A}/import.ts`:
  восемь коротких вызовов общего helper.
- `tests/pcb-analyze/README.md`, `README.md`, `docs/architecture-map.md`,
  `docs/experimental/analyze/README.md`,
  `docs/experimental/analyze/implementation-structure.md`, `CHANGELOG.md`
  и этот отчёт: текущие границы и проверки.

Rust, N-API, загрузчик addon, RoutingBoardV2, экспортёр, алгоритмы placement,
численные режимы, сохранённые JSON/native/evidence/manifest файлы не изменены.
SHA256 используемого `.node` остался
`db16f4da4391a0d68e67119a0a451c8ba159844d7e1dbcd34fbc4df6e4c00096`.

## Подтверждённое поведение

По умолчанию один процесс и четыре ожидающих задания. FIFO одна; каждый слот
переиспользует отдельный workerpool process. Это позволяет через публичный
`terminate()` дождаться выхода именно занятого процесса, сохраняя соседний.
Остановка проверена по PID при синхронной блокировке, отмене и timeout.

Deadline монотонный, с момента принятия, включая очередь и запуск. Проверены
timeout в очереди и в работе, а также отсутствие сброса бюджета при dispatch.
Отменённое ожидающее задание освобождает место. Принятая отмена/timeout исключает
поздний успех; сбойная задача автоматически не повторяется. После остановки и
аварийного exit следующие задачи выполняются.

Проверены `INVALID_REQUEST`, `QUEUE_FULL`, `ABORTED`, `TIMEOUT`, `WORKER_FAILED`,
`DISPOSED`, прогресс без повторных терминальных событий, throwing/rejecting
observers, гонки completion/cancel и completion/dispose, reentrant callbacks,
FIFO/string snapshot, удаление signal listeners и отсутствие unhandled rejection.
Dispose идемпотентен, отклоняет активные/ожидающие/новые задания и ждёт выхода
процессов. После успешного dispose создаётся новое ленивое поколение. Отдельно
инъецирована ошибка завершения placement pool: analysis worker всё равно завершён.

Настоящий addon наблюдался в отличающемся от родителя PID; импорт одного runtime
его не загружал. Родительский event loop продолжал обслуживать таймер. Невалидные
JSON и структура возвращали исходный `imported: false` с diagnostics и фазой
`completed`, а не инфраструктурную ошибку.

## Команды и результаты

| Команда | Результат |
| --- | --- |
| `npm run typecheck` | PASS |
| `npm run build` | PASS; оба новых JS entry point включены |
| `node --import tsx --test --test-concurrency=1 tests/pcb-analysis-runtime.test.ts` | 19 PASS, 0 skipped; финальный запуск с `PCB_BLOCK_BACKEND=cpu`, `PCB_BOARD_BACKEND=cpu`, `PCB_POST_PLACE_BACKEND=cpu` |
| `node --import tsx --test --test-concurrency=1 tests/pcb-analysis-runtime.test.ts tests/public-api.test.ts tests/pcb-board-packer-native.test.ts` | 34 PASS, 0 skipped: 18 runtime на момент запуска и 16 прежних packer/API; последующая добавленная проверка shutdown входит в финальные 19 выше |
| `node --test scripts/test-pcb-analyze.test.mjs` | 4 PASS; в том числе исходные hashes и native inventory восьми плат |
| `npm run test:pcb-analyze -- --all --name import` | 8 completed, 0 failed |
| `node debugging/analysis-runtime/collect-evidence.mjs` | 8 полных отчётов launcher совпали с прямым addon |
| `npm run test:package` | PASS: установленный analysis и прежний placement worker, три режима GPU |
| `git diff --check` | PASS |

После финальных изменений runtime сборка и необходимые проверки повторены;
исходные восемь плат и их hashes не менялись. Rust не изменён, поэтому полная
Rust/placement benchmark серия и native rebuild на этом этапе не повторялись.

## Восемь сохранённых отчётов

Каждая ссылка ведёт к полному `report.json`; рядом лежат `run.json` и `run.log`.
Во всех восьми случаях `imported: true`; совпадают весь accepted, ordered
diagnostics, paths и source IDs с прямым вызовом addon, включая ограничения.

| Плата | Pads / tracks / vias / zones | Diagnostics | Отчёт |
| --- | --- | ---: | --- |
| BandAmpV5 | 29 / 21 / 133 / 2 | 9 | [report](../../../debugging/pcb-analyze/BandAmpV5/runs/2026-10-21T16-25-22-492Z-import-ABbfDm/report.json) |
| ESPower | 172 / 326 / 74 / 2 | 31 | [report](../../../debugging/pcb-analyze/ESPower/runs/2026-10-21T16-25-23-016Z-import-uqJnqd/report.json) |
| NanoScope | 724 / 1135 / 249 / 10 | 72 | [report](../../../debugging/pcb-analyze/NanoScope/runs/2026-10-21T16-25-23-576Z-import-8mmiR2/report.json) |
| NanoSpec | 509 / 823 / 553 / 25 | 76 | [report](../../../debugging/pcb-analyze/NanoSpec/runs/2026-10-21T16-25-24-557Z-import-2KRkpI/report.json) |
| STM32G0_CAN_RS485 | 213 / 472 / 66 / 6 | 53 | [report](../../../debugging/pcb-analyze/STM32G0_CAN_RS485/runs/2026-10-21T16-25-25-346Z-import-rBarHN/report.json) |
| TPS63060DSCR | 39 / 73 / 24 / 6 | 16 | [report](../../../debugging/pcb-analyze/TPS63060DSCR/runs/2026-10-21T16-25-25-935Z-import-zFpU7M/report.json) |
| ThunderF722 | 340 / 494 / 136 / 31 | 111 | [report](../../../debugging/pcb-analyze/ThunderF722/runs/2026-10-21T16-25-26-413Z-import-eWW77h/report.json) |
| USB_PD_Trigger_5A | 65 / 144 / 80 / 2 | 18 | [report](../../../debugging/pcb-analyze/USB_PD_Trigger_5A/runs/2026-10-21T16-25-27-019Z-import-HfnGlk/report.json) |

Машинное свидетельство с полными counts, SHA256 и путями:
[evidence.json](../../../debugging/analysis-runtime/evidence.json).

## Установленный пакет и ограничения

Финальный tarball: `.artifacts/eda-copilot-backend-0.4.0.tgz`, 9342945 bytes.
Изолированный consumer:
`C:/Users/kiril/AppData/Local/Temp/eda-backend-package-z4U9rm`.
Проверка использует только установленный пакет, его JS workers/assets/native,
локальные API fixtures и входные JSON; без исходников backend, tsx и старого
binary. Все восемь отчётов проверены через process worker, сохранены прежние
placement/API/type smoke checks. Режимы: обычный, GPU disabled, CUDA disabled с
несуществующим Vulkan driver. Публичный `/analyze` export не добавлен.

Полный `npm test`/`npm run check` не запускался. macOS/Linux, Node 20/24 и
реальный GPU не проверялись локально; существующий CI выполняет build/check и
package smoke, его конфигурация здесь не менялась и remote CI не запускался.
Очередь не ограничивает Rust RSS; общего CPU-бюджета с placement нет. Deadline
обнаруживается при обслуживании event loop родителя, принудительная остановка
ограничена отдельным grace/exit deadline. Отказ ОС завершить процесс не
имитировался: такой слот сохраняется занятым, dispose возвращает ошибку.
Геометрия контактов, сетки, физические solver, refill, кеш, sessions и DSL
не реализованы. Этап остановлен для приёмки, без коммита.
