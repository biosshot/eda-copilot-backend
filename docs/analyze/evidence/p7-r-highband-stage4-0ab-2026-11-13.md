# P7-R шаг 4.1.0a и 4.1.0b (2026-11-13)

Физика всех моделей остаётся `unverified`. Ничего не закоммичено. Валидатор не менялся.

## 4.1.0a. Купон 10 мм на коммите c5c9999

Отдельный git worktree `wt-c5c9999` (рядом с репозиторием, `node_modules` через junction). Сборка: `CARGO_BUILD_JOBS=4 npm run native:build` (15 мин 37 с, `debugging/p7-r/h/stage4-0a-wt-native-build.log`), затем `npm run build`.

Команда (скрипт `s410.mts` с путём `R=` на worktree, режим cascade):
`RAYON_NUM_THREADS=4 P7_HMATRIX_THREADS=4 L=10 MODE=cascade OUT=debugging/p7-r/h/stage4-0a-c5c9999 npx tsx s410-wt.mts`

| коммит | результат | planarMaxRelativeChange | planarMaxOrder | planarUnconvergedPairs | residualPolicy.limit |
|---|---|---|---|---|---|
| c5c9999 | отказ `CHANNEL_FOIL_EVIDENCE: invalid compact magnetic operator evidence` | 0.0922 | 16 | 165 | 1e-8 (строгий уровень) |

Значения получены временной отладочной печатью в копии валидатора внутри worktree. Печать удалена вместе с worktree. Условие в валидаторе на c5c9999 то же (`x > 2e-3` для строгого уровня). Вывод: отказ воспроизводится на c5c9999, это не регрессия поздних шагов. Продолжили шагом 4.1.0b.

Логи: `debugging/p7-r/h/stage4-0a-c5c9999/run.log`, `run2-debug.log`. Worktree удалён (`git worktree remove --force`, `git worktree prune`), junction снят до удаления, сборки в worktree не оставлены.

## 4.1.0b. Переключатель `P7_PAIR_ESCALATE=1` (по умолчанию выключен)

- `native/analyze/src/extraction/local/planar_operator.rs`, `pair_integral_compact`: пара, у которой после порядка 16 относительное изменение больше 2e-3 (`PAIR_NOISE_ACCEPT_CHANGE`), пересчитывается той же лестницей порядков (`PAIR_COMPACT` с `max_order` 64). Если результат лучше по изменению, он заменяет прежний; иначе остаётся прежний. Остальные пары (быстрые пути, сошедшиеся до 16) не затронуты. Выключенный переключатель: код идёт прежним путём.
- Счётчики: атомарные (пары, улучшенные пары, наносекунды). Одна строка stderr на сборку оператора: `P7_PAIR_ESCALATE escalatedPairs=N improvedPairs=M addedCpuSeconds=T maxOrder=64`. `addedCpuSeconds` это сумма времени потоков, не стенное время.
- `foil_reduced.rs`: только вызов `escalate_report()` после строки `P7_COMPACT_STAGE stage=training-end` (первая версия печатала после H-матрицы, но купон 10 мм идёт без H-матрицы, поэтому строки не было; пришлось собрать ещё раз).
- Сборки: `npm run native:build` дважды, 7 мин 44 с и 7 мин 26 с (`stage4-0b-native-build.log`, `stage4-0b-native-build2.log`).

### Проверка 1: купон 10 мм cascade, строгий уровень, переключатель включён

Команда: `P7_PAIR_ESCALATE=1 RAYON_NUM_THREADS=4 P7_HMATRIX_THREADS=4 L=10 MODE=cascade OUT=debugging/p7-r/h/stage4-0b-coupon-on npx tsx s410.mts`.

| величина | c5c9999 / переключатель выключен | переключатель включён |
|---|---|---|
| статус | отказ CHANNEL_FOIL_EVIDENCE | complete |
| planarMaxRelativeChange | 0.0922 | 1.26e-3 |
| planarMaxOrder | 16 | 64 |
| planarUnconvergedPairs (изменение > 1e-5) | 165 | 165 |
| эскалировано пар / улучшено | - | 27 / 27 |
| добавлено времени | - | 2.34 CPU с (training-end 0.5 с -> 2.8 с) |

Лог: `debugging/p7-r/h/stage4-0b-coupon-on.log`, результат `stage4-0b-coupon-on/cascade-10.json`. Купон проходит строгий режим. 165 пар по-прежнему не достигают цели 1e-5, но изменение у всех не выше 2e-3.

### Проверка 2: `diag_compact_kvl_100k_vs_1m`, переключатель выключен

Команда: `RAYON_NUM_THREADS=4 P7_HMATRIX_THREADS=4 CARGO_BUILD_JOBS=4 cargo test -j 4 --manifest-path native/analyze/Cargo.toml --lib diag_compact_kvl_100k_vs_1m -- --ignored --nocapture`. Лог `debugging/p7-r/h/stage4-0b-coupon-off.log`, сравнение с `stage3c-coupon-off.log`.

- Тест проходит, 96.9 с (stage 3c: 153.8 с, нагрузка машины была другая).
- История `maxKvlRelative` и `maxKvlUncertaintyRelative` по шагам сетки совпадает с `stage3c-coupon-off.log` до 3 значащих цифр: 100 кГц 6 шагов (1.14e-3 ... 9.99e-10), 1 МГц 10 шагов (1.15e-2 ... 1.15e-9).
- Строки `P7_PAIR_ESCALATE` в логе нет.

### Проверка 3: TPS63060DSCR band-step 3 МГц, переключатель включён

Команда (как в `stage3e-seq4.sh`, драйвер `drive3d.py` -> `run_command`, 4096 МиБ, 900 с, холодный кэш): `. debugging/p7-r/h/stage3e-env.sh; P7_PAIR_ESCALATE=1 python drive3d.py 3000000 stage4-0b-tps-3000000`. Флаги этапа 3: `P7_FOIL_SHEET=4 P7_BARREL_NOISE=1 P7_BARREL_SHEET=2 P7_ENRICH_INDUCTIVE=5 P7_ENRICH_GMRES_MAX=120 P7_HMATRIX_THREADS=4 RAYON_NUM_THREADS=4 P7_HMATRIX_CACHE=1 P7_MAGNETIC_PHASE_PROFILE=1`. Логи: `debugging/p7-r/h/stage4-0b-tps-3000000/`, `stage4-0b-tps-3000000.rec`; прежний прогон `stage3e-tps-3000000/`.

| величина | stage3e (выкл.) | с переключателем |
|---|---|---|
| статус | model | model |
| неопределённость оператора (maxCompactKvlUncertaintyRelative) | 1.193e-2 | 1.195e-2 |
| H-матрица: maxRelativeChange | 0.0974 | 1.997e-3 |
| H-матрица: maxOrder | 16 | 64 |
| эскалировано пар / улучшено | - | 3326 / 3326 (из 11.1 M точных пар) |
| добавлено времени на пары | - | 17.9 CPU с |
| время сборки H-матрицы | 135.3 с (cpu 540 с) | 120.9 с (cpu 483 с) |
| время всего | 166.8 с | 146.7 с |
| пик private | 1063 МиБ | 1068 МиБ |
| итерации (шаги сетки) | 5 (последняя 4) | 5 (последняя 4) |
| итоговый KVL | 2.43e-4 | 2.43e-4 |
| unconvergedPairs H-матрицы | 389253 | 389206 |

Вывод: эскалация снижает наибольшее изменение пары с 0.097 до 2.0e-3 и почти ничего не стоит (18 CPU с на 4 потоках; разница стенного времени в пределах шума машины). Неопределённость оператора 1.2e-2 не меняется: она определяется не пар-квадратурой. Порог 1e-2 (шаг 2.4) этим не решается.

## Остановки

Нет. Купоны 10/20 мм шага 4.1.0 не запускались.
