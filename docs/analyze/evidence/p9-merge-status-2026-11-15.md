# P9 после слияния: статус, 2026-11-15

Ветка `feat/p9-thermal`, merge `d06a183`. Проверки запускались в этом worktree по одной тяжёлой команде за раз (`RAYON_NUM_THREADS=2`, `CARGO_BUILD_JOBS=2`).

## Прогоны этой сессии

| Проверка | Результат | Причина / примечание |
| --- | --- | --- |
| `tests/pcb-analysis-thermal.test.ts` | 10/10 после сборки | Сначала 5 падений: отсутствовал `dist/` (исправлено `npm run build`) и бинарник аддона (см. ниже). |
| `tests/pcb-analysis-thermal-audit.test.ts` | 5/5 | |
| `captured_controller_steady_geometry_budget_regression` (merged `d06a183`) | FAIL: `thermal_board.rs:602`, `relative <= 0.02` | Значение 0.0950368 на последнем шаге (edge=1, deviation 0.005). |
| Та же проверка на `8112bac` (`analyze-thermal`, ветка `codex/analyze-thermal`) | FAIL, то же место | Значение 0.0950486. Падение существовало до слияния. |

Регрессия не введена слиянием: разница относительного изменения между ветками 1.2e-5, но число общих точек поля отличается (core: 13362 в исходной ветке, 10939 в слитой). Тест не редактировался. Причина расхождения по числу точек не выяснялась.

Сборка аддона: `npm run native:build` падает с `--locked`. Причина: слияние добавило `sha2 = "=0.10.9"` в `native/analyze/Cargo.toml`, а `native/node-addon/Cargo.lock` не обновлён. Аддон собран вручную `cargo build --release --offline` (без `--locked`) и скопирован в `native/node-addon/eda-copilot-node.win32-x64-msvc.node`. Изменение `native/node-addon/Cargo.lock` осталось незакоммиченным и нужно для сборки.

Результаты слияния, как переданы: lib thermal 13/0, `tests/thermal.rs` 11/0, `thermal_mesh` 3/0, `thermal_board` 14/1 (падение выше), `thermal-field-comparison` 8/0, `thermal-stationary-reaction` 2/0.

## Обязательства P9 (`docs/analyze/plans/implementation-plan.md`, раздел P9)

Легенда: **готово и проверено** / **реализовано, не проверено** / **нет**.

| Обязательство | Статус | Где в коде | Тесты и evidence |
| --- | --- | --- | --- |
| Слоистая теплопроводность (медь/диэлектрик/vias) | готово и проверено | `native/analyze/src/thermal/board.rs`, `mesh.rs` | `thermal_board`: `homogeneous_slab_and_multilayer_harmonic_series`, `high_contrast_narrow_copper_spreading_and_mesh_alignment`, `finite_via_barrel_conductance_mass_and_loss_transfer`; [thermal-verification.md](thermal-verification.md) |
| Компактные package thermal paths | реализовано, частично проверено | `src/analyze/analysis-dsl/thermal-board.ts:23-26`, `schema.ts` (`thermal-path`, `thermal-network`) | покрыто `tests/pcb-analysis-thermal.test.ts` (T1 пример) |
| Стационарная задача | готово и проверено | `native/analyze/src/thermal/solve.rs` | `thermal.rs`: `analytic_rc_and_temporal_refinement`, `two_node_modal_reference`; `thermal_board`: `captured_tps_steady_real_geometry_regression` |
| Неявная адаптивная временная задача | частично | `solve.rs` (MOSFET-ориентированный интегратор не найден grep-ом) | адаптивность в тестах касается уточнения сетки (`adaptive_refinement_*`), а не шага по времени; явного неявного интегратора и адаптивного шага по времени не нашёл, проверить отдельно |
| Заданные мощности и карты DC-потерь | готово и проверено | `src/analyze/analysis-dsl/thermal-loss-transfer.ts` | `thermal_board`: `finite_via_barrel_conductance_mass_and_loss_transfer`; `pcb-analysis-thermal.test.ts`: production DC loss transfer (проходит) |
| Температура среды и профили | готово и проверено | `thermal-executor.ts` (`ambientC`, `initialC`, `initial`) | `thermal.rs`: `events_pulse_ambient_and_cooling_switch`, `fixed_coordinate_origin_preserves_varying_ambient_nonlinear_cooling_energy_and_continuation` |
| Initial state | готово и проверено | `thermal-executor.ts` (`steady` / data) | `thermal.rs`: `continuation_compatibility_and_budgets_recovery` |
| Радиатор (radiator) | нет | в коде нет ни одного совпадения | — |
| Heatsink с теплоёмкостью | реализовано, частично проверено | `schema.ts` (`heatsink`), `thermal-board.ts:41-42` | `tests/pcb-analysis-thermal-audit.test.ts`, board-ожидания `tests/pcb-analyze/*/thermal*.expectations.json`; отдельного unit-теста на heatsink нет |
| TIM / тепловой интерфейс | реализовано, частично проверено | `schema.ts` (`thermal-interface`), `board.rs` | используется board-сценариями BandAmpV5 и TPS (`tests/pcb-analyze/*/thermal*.js`); отдельного unit-теста на TIM нет |
| Контакты и теплоёмкость | готово и проверено | `board.rs` | `thermal_board`: `missing_material_contact_source_and_geometry_reject`, `spatial_source_conservation_and_time_capacity`; `thermal.rs`: `floating_missing_capacity_invalid_table_and_duplicate_power_reject` |
| Пассивное охлаждение (табличный закон) | готово и проверено | `schema.ts` (`cooling`), `thermalCooling` в `thermal-executor.ts` | `thermal.rs`: `table_nonlinear_known_steady_and_range_guard` |
| Активное охлаждение (обдув) | нет как отдельная модель | `grep fan/airflow/forced` в native и DSL: 0 совпадений | только общий `cooling` с табличным законом; явного обдува нет |
| Разделение junction/case/board/ambient | частично | температуры по узлам компонентов в `thermal-board.ts`, ambient есть | отдельного case-узла в DSL и тестов нет |
| `thermalNetwork` без платы | реализовано и проверено | `thermal-executor.ts:43` (`c.network`, ветка без `board`) | DSL-запуск: `tests/pcb-analysis-thermal.test.ts:88`; валидация: `tests/analysis-dsl.test.ts:209` |
| Сеточная и временная сходимость | готово и проверено | `refinement.rs` | `thermal_mesh`: 3/0; `thermal_board`: `adaptive_refinement_uniform_analytic_and_independent_final_field`; `thermal-field-comparison` 8/0 |
| Корпус MOSFET / радиатор / MCU с независимыми данными | нет | — | `captured_*` тесты используют TPS и STM32G0 captured board, а не MOSFET/радиатор |

## DSL `thermal()`

Исполнитель есть. [dsl-implementation-status.md](../reference/dsl/dsl-implementation-status.md) строка 28 («DSL-исполнителя нет») устарела: `executor.ts:57-59` подключает `createThermalExecutor` из `thermal-executor.ts`. Строка 81 про `thermal-network`, `heatsink`, `cooling` тоже устарела для путей, которые выполняет исполнитель; документ нужно обновить отдельно.

## Открыто

- Радиатор, отдельный case-узел, активное охлаждение (обдув) и MOSFET/радиатор/MCU корпус с независимыми данными отсутствуют.
- `captured_controller_steady_geometry_budget_regression` падает и до слияния; порог 2% не выполняется на последнем шаге сетки.
- Адаптивный шаг по времени и неявный интегратор не подтверждены.
- Аддон `Cargo.lock` в `native/node-addon` нужно обновить и закоммитить отдельно.
