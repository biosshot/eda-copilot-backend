# P7-R шаг 4.2, прогоны (2026-11-14)

Физика `unverified`, ярус approximation, эталона нет. Ничего не закоммичено. Логи: `debugging/p7-r/h/stage4-2-1/`, скрипты `debugging/p7-r/h/stage4-2-1-scripts/`. 4 ядра (RAYON_NUM_THREADS, P7_HMATRIX_THREADS, CARGO_BUILD_JOBS = 4), чистый кэш на конфигурацию, окружение 4.1.6в (`P7_HIGHBAND_CUTS=1 P7_FIELD_INTERFACE_STEP=1 P7_FIELD_REFINE=1`, глубина 3).

## 4.2.1, порог размера и канал RF_IN

### Код `P7_BAND_SCREEN`
`P7_BAND_SCREEN=<kD>` (0 < kD <= 0.5, иначе `BOARD_FOIL_SCOPE`) заменяет множитель 0,1 в квазистатической проверке размера `MIXED_FOIL_BAND` (`mixed_conductor.rs`, функции `band_screen_kd`, `band_kd_from`, `band_screen_record`). Действует только при `P7_HIGHBAND_CUTS=1`: «при включённых разрезах» понимается именно так. Без переключателя поведение прежнее. Запись `provenance.bandScreen` (kD, thresholdHz, sizeM, epsilonR, maximumHz, electricalSize, status approximation, statusNote) пишется только при активном переключателе; валидатор `validateBandScreen` в `src/analyze/board-foil-validation.ts`. Излучение и запаздывание в области не проверены.

### Тесты
`cargo test -j 4 --release --lib hybrid_rf_policy`: 1 пройден (сборка 2 мин 45 с). `npx tsx --test tests/analyze-board-foil-cache.test.ts`: одна известная старая ошибка (Missing expected exception).

### Паритет
Купон 10 мм, 100 МГц, окружение 4.1.6в без `P7_BAND_SCREEN`: S побитно равна `stage4-1-6c/d3-L10`. С `whole4` не совпадает (иная конфигурация того прогона, не разбиралось).

### Купон 10 мм, kD 0,5, цельный и каскад (блоки + сечение 2D)
Без переключателя на 1 ГГц цельный купон отказывает `LOCAL_MIXED_FOIL_BAND` (размер 0,447 > 0,1, предел 223,6 МГц); с kD 0,5 проходит. Все прогоны `complete`.

| f, МГц | наиб. σ цельного | \|S12-S21\| цельного | задержка цельного, пс | задержка каскада, пс | разница, пс (%) | max \|ΔS\| |
|---|---|---|---|---|---|---|
| 300 | 1 - 2,6e-12 | 3,4e-16 | 65,37 | 62,45 | -2,92 (-4,5) | 0,0056 |
| 500 | 1 - 8,9e-12 | 1,1e-16 | 65,19 | 61,92 | -3,27 (-5,0) | 0,0103 |
| 1000 | 1 - 5,4e-11 | 5,6e-17 | 64,60 | 61,17 | -3,43 (-5,3) | 0,0213 |

Пассивность и взаимность каскада: σ не выше 1 (1 - 7e-5 на 1 ГГц), \|S12-S21\| не выше 2,8e-16. Время цельного около 8 с, память около 270 МиБ. Расхождение каскада с цельным растёт с частотой; причины (радиация, запаздывание, секция 2D) не разделялись, эталона нет.

### BandAmp RF_IN
Планировщик отказал на `createBoardChannelPlanner` (`s421b.mts`, boundaryFraction 0,05): `CHANNEL_CORRIDOR_UNSUPPORTED`, «CHANNEL_CORRIDOR: Finite return closure needs distributed interface coordinates across multiple corridors». Блоки, счёт узлов и каскад RF_IN не запускались (остановка по заданию). Файл: `debugging/p7-r/h/stage4-2-1/probe/planner-refusal.json`.

## 4.2.2, L на метр, ключ кэша, тесты

Физика `unverified`, ярус approximation, эталона нет. Логи: `debugging/p7-r/h/stage4-2-2/`, скрипты `debugging/p7-r/h/stage4-2-2-scripts/` (`s422.mts` — копия `s413h.mts` с записью `provenance.bandScreen`; `run.sh`; `lc.mjs` — извлечение через `abcd.mjs` и сравнение с секцией 2D). 4 ядра, окружение 4.2.1 (`env.sh`) и `P7_BAND_SCREEN=0.5`, чистый кэш на прогон. Купоны 10 и 5 мм целиком (`MODE=full`), секция 2D того же вызова, что в каскаде (`NOLOCAL=1`, `MODE=blocks`, потери при f, толщина скин-слоя 10 мкм).

### L и C на метр: 3D по разности 10-5 мм и секция 2D

| f, МГц | L 3D, нГн/м | L 2D | dL, % | C 3D, пФ/м | C 2D | dC, % | задержка 3D, пс/мм | задержка 2D | d, % |
|---|---|---|---|---|---|---|---|---|---|
| 50 | 459,5 | 442,0 | -3,8 | 82,13 | 79,18 | -3,6 | 6,161 | 5,916 | -3,7 |
| 300 | 456,3 | 433,1 | -5,1 | 81,66 | 79,18 | -3,0 | 6,129 | 5,856 | -4,1 |
| 500 | 452,8 | 428,0 | -5,5 | 80,80 | 79,18 | -2,0 | 6,114 | 5,822 | -3,7 |
| 1000 | 438,8 | 422,6 | -3,7 | 76,85 | 79,18 | +3,0 | 6,064 | 5,785 | -0,4 |

L и C 3D взяты в первом порядке (Zs, Ys разности ABCD), «задержка 3D» из собственных значений (точная). Для 2D задержка = sqrt(LC). R на метр: 3D 21,3 / 30,1 / 39,8 / 60,3 Ом/м, 2D 21,5 / 41,3 / 55,4 / 78,8. Определители ABCD равны 1 до 1e-15.

Вывод: на 300-500 МГц разрыв задержки каскада (-4,5...-5,3 %) несёт в основном L (2D ниже на 5,1-5,5 %), C 2D ниже на 2-3 %; на 1 ГГц L и C 2D расходятся в разные стороны (-3,7 % и +3,0 %), поэтому задержка 2D близка к 3D (-0,4 %), а разрыв каскада 1 ГГц (-5,3 %) тогда не объясняется погонной задержкой секции и остаётся за блоками и стыками. C 2D статична (79,18 пФ/м), а C 3D падает с частотой (82,1 до 76,8); L 2D тоже растёт-падает иначе. Причины разности (излучение, запаздывание, определение C по ABCD первого порядка при kD 0,2-0,45) не разделялись.

### provenance.bandScreen (P7_BAND_SCREEN=0.5, status approximation)

| купон | f, МГц | kD | thresholdHz | sizeM | epsilonR | maximumHz | electricalSize |
|---|---|---|---|---|---|---|---|
| 10 мм | 50 | 0,5 | 1,1181e9 | 0,0106684 | 4 | 5e7 | 0,022359 |
| 10 мм | 300 | 0,5 | 1,1181e9 | 0,0106684 | 4 | 3e8 | 0,134156 |
| 10 мм | 500 | 0,5 | 1,1181e9 | 0,0106684 | 4 | 5e8 | 0,223593 |
| 10 мм | 1000 | 0,5 | 1,1181e9 | 0,0106684 | 4 | 1e9 | 0,447186 |
| 5 мм | 50 | 0,5 | 2,0823e9 | 0,00572839 | 4 | 5e7 | 0,012006 |
| 5 мм | 300 | 0,5 | 2,0823e9 | 0,00572839 | 4 | 3e8 | 0,072035 |
| 5 мм | 500 | 0,5 | 2,0823e9 | 0,00572839 | 4 | 5e8 | 0,120058 |
| 5 мм | 1000 | 0,5 | 2,0823e9 | 0,00572839 | 4 | 1e9 | 0,240116 |

Полные записи: `debugging/p7-r/h/stage4-2-2/L10/full-a0.15.json`, `L5/full-a0.15.json`. Все 8 прогонов `complete`.

### Ключ кэша

`LocalModelCache.key` (`src/analyze/local-model-cache.ts`) добавляет в идентичность поле `resultSwitches` из установленных переключателей списка `RESULT_SWITCHES` (значение как строка). Без единого `P7_*` из списка ключ байт-в-байт прежний (поле не добавляется; проверено тестом по литералу канонической строки). Ключ общий для извлечения, секции, канала, частотного аудита и board-foil.

Включены (34): P7_BAND_SCREEN, P7_BARREL_MODEL, P7_BARREL_SHEET, P7_CROSS_TET_TOL, P7_ENRICHMENT_CONTINUATION, P7_ENRICH_GMRES_MAX, P7_ENRICH_INDUCTIVE, P7_ENRICH_MAX, P7_FIELD_INTERFACE_STEP, P7_FIELD_REFINE, P7_FIELD_REFINE_COUNT_ONLY, P7_FIELD_REFINE_LEVELS, P7_FIELD_REFINE_MAX_XY, P7_FOIL_SHEET, P7_HIGHBAND_CUTS, P7_LOCAL_FAST, P7_MESH_COARSEN, P7_MESH_MERGE, P7_MESH_MERGE_ANGLE, P7_MESH_MERGE_TOL_MM, P7_MESH_QUALITY_ANGLE, P7_MESH_SIMPLIFY, P7_MESH_SNAP_MM, P7_PAIR_ESCALATE, P7_PLANAR_HMATRIX, P7_POINT_CONTACT_REFUSE, P7_RECIPROCITY_POLICY, P7_ROI_BUDGET_MIB, P7_ROI_CORRIDOR, P7_ROI_ELECTRICAL, P7_ROI_FALLBACK, P7_ROI_FORCE_CORRIDOR, P7_ROI_RECALIBRATE, P7_SECTION_FOIL.

Исключены: диагностика и трассы (P7_MAGNETIC_TRACE, P7_PHASE_TRACE, P7_MAGNETIC_PHASE_PROFILE, P7_MAGNETIC_SLOW_PAIR_PATH, P7_PAIR_PROFILE, P7_HMATRIX_PAIR_PROBE, P7_ENRICH_PROBE, P7_LAPLACIAN_DIAG/DUMP, P7_CROSS_DUMP, P7_DUMP_GEOMETRY/EXIT, P7_VIA_PRISM_CENSUS, P7_VIA_COVER_CONTROL, P7_MESH_COARSEN_BASELINE, P7_SECTION_FOIL_TRACE, P7_COMPACT_TT_PAIR_TRACE, P7_AUTO_FOIL_AUDIT, P7_BLOCK_LIFECYCLE, P7_CACHE_DEBUG) не меняют значения; потоки и ресурсы (P7_HMATRIX_THREADS, P7_HMATRIX_CACHE, P7_HMATRIX_CACHE_DIR, P7_MAGNETIC_PAIR_CACHE_MIB, P7_MAGNETIC_COVER_CACHE_MIB, P7_VERIFY_SECONDS, P7_VERIFY_MEMORY_MIB) не входят в физический результат; тестовые и стендовые входы (P7_BENCH_*, P7_SAVED_PROBLEM, P7_TEST_EVIDENCE_DIR, P7_PAIR_OUTPUT, P7_CROSS_SAMPLES, P7_BARREL_TEST_WALL_SECONDS). Уже входили в ключ по имени и не повторяются: P7_MAGNETIC_PROFILE/PRECISION/AXIAL_TETS/BATCH_COLUMNS/NEAR_BOUND/NEAR_THREADS, P7_COMPACT_PLANAR/FULL_MIXED/PL_THREADS, P7_SECTION_HINTS, P7_HYBRID_REGIME_AUDIT. Спорное: P7_LOCAL_FAST (плотная нецертифицированная таблица) включён осторожно.

Тест: `result-changing P7 switches isolate every cache key...` в `tests/analyze-board-foil-cache.test.ts` (два разных значения каждого переключателя дают разные ключи, снятие возвращает исходный, диагностика ключ не меняет). Нативная сборка не нужна (ключ считается в TS); `npm run typecheck` и `npm run build` прошли.

### Тесты (4 ядра, P7_* не заданы, `cargo test -j 4 --release --lib <фильтр> -- --test-threads=4`)

| фильтр | результат |
|---|---|
| mixed_conductor | 19 пройдено, 2 известных: `thin_wall_barrel_potentials_match_the_electric_field_trace` (138 != 140), `imported_via_two_drives_and_missing_contact` (LOCAL_DEADLINE) |
| copper_skin_profile | 4 пройдено |
| foil_reduced | 30 пройдено, 2 ошибки, обе воспроизведены на чистом HEAD 3890bbc (рабочее дерево `git worktree`, 29/2): `compact_planar_tests::local_hierarchy_encloses_signed_sparse_action` (столбец 0 строка 0, actual 2,1786728142692e-8 против direct 2,1786728142605e-8, bound 6e-23), `native_memory_tests::partitioned_plan_omits_only_unused_global_sparse_assembly` (28728377 != 28344377) |
| planar_trace | 9 пройдено, 1 известная: `production_profiled_via_field_trace_covers_mixed_coordinates` (458/462) |
| layered_spatial | 5 пройдено |
| polygon_section | 3 пройдено |
| losses | 5 пройдено |
| `npx tsx --test tests/analyze-board-foil-cache.test.ts` | 13 из 14 пройдено; 1 известная: `compact magnetic evidence is checked without bypassing ownership or power gates` (Missing expected exception) |

Новых отказов нет. Логи `t-*.log`.
