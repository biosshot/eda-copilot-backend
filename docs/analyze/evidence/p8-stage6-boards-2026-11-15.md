# P8 этап 6: board pi() на трёх реальных платах (2026-11-15)

Итог: ни одна плата не дала частотного результата. Все три прошли сборку с объявленными паразитами
рельса и дошли до решателя, но упали на разных ограничениях. Ни один из них не является падением кода
(TypeError и т.п.). Два из трёх (лимиты памяти и бюджета) выглядят как скрытые пределы и требуют
решения владельца. Код решателя этим проходом не менялся.

## Результаты (бюджет 1800 с / 6144 MiB, RAYON_NUM_THREADS=4, P7_HMATRIX_THREADS=4, один прогон на плату)

| Плата | Портов | Статус | Код | Где упало | f_hi | Полоса | |Z| 1 kHz / 1 MHz / f_hi | Резонансы | bandFit | dcStitch | wall, s | peak MiB (дерево) |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| TPS63060DSCR | 6 | failed | LOCAL_FOIL_REDUCTION_RESIDUAL | AC-экстракция, residual KCL/KVL (`native/analyze/src/extraction/local/foil_reduced.rs:1957`) | - | - | - | - | - | - | 602.0 | 1031 |
| STM32G0_CAN_RS485 | 13 | failed | LOCAL_MEMORY_LIMIT | ROI-fallback, ограничение 2 GiB (`native/analyze/src/extraction/local/roi_fallback.rs:315`) | - | - | - | - | - | - | 7.3 | 279 |
| BandAmpV5 | 5 | failed | CHANNEL_RETURN_UNRESOLVED, DC: PI_DC_STITCH_UNAVAILABLE | AC-экстракция, граница тока 0.27 > 0.1 на сжатой области (`src/analyze/auto-foil-response.ts:135`) | - | - | - | - | - | недоступно | 157.6 | 1064 |

Для всех трёх таблица частот и резонансов пуста: решение не вычислено.

## Блокеры

1. TPS63060DSCR: `LOCAL_FOIL_REDUCTION_RESIDUAL` на 599 с (`native/analyze/src/extraction/local/foil_reduced.rs:1957`,
   сообщение `unreduced KCL=2.0e-4, KVL=1.18e-2; foil coverage unverified`). Это отказ проверки невязки,
   заявленный в CHANGELOG как известное ограничение этапа 3 (окно выше 5 MHz). Не падение кода.
2. STM32G0_CAN_RS485: `LOCAL_MEMORY_LIMIT` за 4 с. Для ROI-решения используется константа
   `DSL_DEFAULT_MEMORY_MIB = 2048` (`native/analyze/src/extraction/local/roi_fallback.rs:238`, применяется на строке 245),
   которая не учитывает бюджет запроса 6144 MiB, пока не задана переменная `P7_ROI_BUDGET_MIB`.
   Минимальный коридор 3 мм требует около 4.6 GiB, полный ROI 4.8 GiB. Это скрытый предел того же рода, что и
   предыдущие (не патчился).
3. BandAmpV5: `CHANNEL_RETURN_UNRESOLVED` (`src/analyze/auto-foil-response.ts:135`): граница тока 0.27 выше
   предела 0.1 на области, сжатой бюджетом; увеличение области даёт ту же сетку. Параллельно DC-конец
   недоступен: `LOCAL_BOARD_CUT_POLICY: ROI cuts copper; explicit insulated-cut boundary policy required`
   (требуется явная политика изолированного разреза). Это контрактное отказание, а не ошибка кода.

## Что сделано в этом проходе

- Пересборка нативного модуля (`CARGO_BUILD_JOBS=2 npm run native:build`, exit 0) после правки `board_dc.rs`.
- Правка TS `pi-frequency-executor.ts` (`min(budget, 4096)` для P6-оценки) не сняла блокер 1-го уровня
  COUPLED_BUDGET, но TPS на ней дошёл до AC-экстракции и упал уже на LOCAL_FOIL_REDUCTION_RESIDUAL.
- Лишние параллельные запуски TPS, обнаруженные в ходе работы, остановлены перед новым прогоном.
  Итоговые результаты получены из одиночных прогонов.
- Прогоны STM32 и BandAmp шли последовательно, один за другим.

## Объявленные паразиты (гипотезы, не измерены)

Стек 1.6 мм, 2 слоя, Cu 35 мкм, диэлектрик 1.53 мм, eps_r 4.2. Значения помечены в фикстурах.

- TPS63060DSCR: источник U1.9/U1.7 (Rout 50 mOhm); нагрузка U1.9/U1.11 (единственный VOUT-вывод U1 - 9);
  C3, C4, C5 (22 uF C0805, ESR 3 mOhm, ESL 1 nH); J2 как резистор 10 mOhm.
- STM32G0_CAN_RS485: источник U2.5/U2.2 (Rout 200 mOhm); нагрузка U3.4/U3.7; C8, C11 (10 uF, ESR 5 mOhm, ESL 1 nH);
  C13, C22 (1 uF, ESR 20 mOhm, ESL 0.6 nH); C9, C10, C12, C14, C15, C19, C21 (100 nF, ESR 10 mOhm, ESL 0.6 nH).
- BandAmpV5: источник H1.1/H1.2 (Rout 100 mOhm); нагрузка U1.1/U1.2; C3 (470 pF, ESR 50 mOhm, ESL 0.5 nH),
  C4 (100 nF, ESR 10 mOhm, ESL 0.6 nH), C5 (1 uF, ESR 20 mOhm, ESL 0.6 nH).

Физика: `unverified`. Точность не оценивалась.

## Команды

    CARGO_BUILD_JOBS=2 npm run native:build
    powershell -NoProfile -ExecutionPolicy Bypass -File debugging/p8-stage6/run-measured.ps1 -Name <плата> -Board <board.json> -Fixture <fixture.js>

Внутри: `RAYON_NUM_THREADS=4`, `P7_HMATRIX_THREADS=4`, `npx tsx debugging/p8-stage6/run-pi.ts`.
Бюджет: `.budget({maxSeconds:1800,maxMemoryMiB:6144})`. Сетка 1 kHz - 40 MHz, 30 точек, log. `P7_VERIFY_SECONDS`
и `P7_ROI_BUDGET_MIB` не задавались.

Память: сумма WorkingSetSize дерева процессов (`cmd.exe` и потомки), опрос 500 мс. Поле `exitCode` в
`memory.json` равно null (запуск через `cmd.exe`); статус берётся из `summary.json`.
Выходы: `debugging/p8-stage6/<плата>/{summary.json,result.json,memory.json,stdout.txt,stderr.txt}` (git-ignored).

## Что нужно решить

1. Лимит 2 GiB для ROI (`roi_fallback.rs:238`) и бюджет 6144 MiB для STM32. Решение: учитывать бюджет запроса.
2. Для BandAmp: явная политика изолированного разреза (`LOCAL_BOARD_CUT_POLICY`) и/или больший бюджет
   для области. Иначе граница тока не сходится.
3. Для TPS: residual `LOCAL_FOIL_REDUCTION_RESIDUAL` выше 5 MHz - это предел точности экстракции, а не лимит.
