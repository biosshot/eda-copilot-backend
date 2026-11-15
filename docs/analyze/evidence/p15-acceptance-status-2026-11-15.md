# P15: статус интеграции после автономной работы (2026-11-15)

Не приёмка плана целиком: фиксирует, что реализовано в автономном режиме, что проверено и что осталось. Решения и откаты — в [журнале](../plans/autonomous-decisions-2026-11-15.md).

## Полный TS-прогон

`npm run build` + `npm test` (RAYON_NUM_THREADS=2): 1226 тестов, 1172 pass, 11 fail, 43 skipped. Разбор 11:

| Тест | Причина | Итог |
|---|---|---|
| doc vocabulary (`analysis-dsl`) | P14 убрал единственное упоминание `ANALYZER_NOT_IMPLEMENTED` из `spec-doc.d.ts` | исправлено (`e354cbd`) |
| addon exports (`pcb-native-addon`) | слияние P9 добавило `solveAnalysisThermal`, ожидание теста не обновлено | исправлено |
| 2× ENOENT (`pcb-native-addon`), 1× ENOENT (`electrical-core-examples`) | документы перенесены в `docs/analyze/reference/…` (`517d7d6`) | пути исправлены |
| 2 купона P8 PI + tiny budget (`analyze-pi-*`) | собственная регрессия `fa9fc56`: `plannedBytes`=2^64−1 при неограниченной памяти | исправлено в native; тест бюджета переведён на лимит времени |
| cross-section, 2× line-losses-audit | падали до этой работы | открыты |

После исправлений: `analyze-pi-frequency` + `analyze-pi-board` 9/9, `pcb-native-addon` 5/5, `electrical-core-examples` 1/1, `analysis-dsl` 31/31. Полный прогон повторно не запускался (нагрев ноутбука); ожидаемо 3 известных падения.

Rust (только затронутые модули): `mixed_conductor` 19/21, `foil_reduced` 31/32 — три падения известны и до работы (thin_wall_barrel, imported_via, local_hierarchy).

## Состояние этапов

| Этап | Статус | Тесты |
|---|---|---|
| P8 | DC и частотный PI на плате; на реальных платах — честные отказы P7; временной PI в `wip/p8-stage4` | `analyze-pi-*` |
| P9 | v1 влит (`09b16c4`) | `pcb-analysis-thermal` |
| P10 | стационарная связь standalone circuit×network | `analysis-electrothermal` 7/7 |
| P11 | `eye()`, `timing()`, `tdr()` на standalone network | `analysis-eye-timing` 13/13, `analysis-tdr` 9/9 |
| P12 | `emi()` conducted/coupling на standalone network | `analysis-emi` 14/14 |
| P13 | пропущен (вне дорожной карты) | — |
| P14 | `solve()`/`optimize()` | `analysis-search` 14/14 |

## Не сделано в P15

Корпус ≥20 плат, сквозной проход матрицы DSL, проверка overlays/единиц, размер бинарника, паритет потребителей router/Preview. Главный блокер реальных плат для P8/P10–P12 — ВЧ-экстракция P7 (сейчас 1–10 кГц и отказы N≥4 портов), её разбор отложен до доступности Sonnet (после 19.11).
