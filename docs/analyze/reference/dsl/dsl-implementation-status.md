# DSL: статус реализации

Живой документ. Обновлён **2026-11-04**. Подключение исполнителя не означает физическую приёмку P7.
Спецификация — [dsl-reference.md](dsl-reference.md). Здесь только факт: что подключено в коде.

Обозначения: **да** — работает; **нет** — объявлено в схеме, но исполнителя нет;
**частично** — работает с ограничениями, они перечислены.

Если анализатор недоступен, `prepare()` выдаёт `ANALYZER_NOT_IMPLEMENTED`, а `run()` отклоняет
задачу. Источник статуса — гейт `available` в `src/analyze/analysis-dsl/prepare.ts:205` и
диспетчер `src/analyze/analysis-dsl/executor.ts`.

## Анализы

| Раздел DSL | В схеме | В коде | Ограничение |
|---|---|---|---|
| `runAnalysisDsl` | да | **да** | Изолированный процесс, отмена, лимиты |
| `analysis({ board })` | да | **да** | Снимок платы неизменяем |
| `geometry()` | да | **да** | connectivity / clearance / edge-clearance |
| `dc()` | да | **да** | плата, `circuit`, одиночный `conductor` |
| `pi()` | да | **частично** | с явным `circuit`; на плате (без `circuit`): DC-точка `frequencyHz(0)` (P8 этап 2) и частотный режим `frequencyHz(f>0)` / `frequency({...})` (P8 этап 3, `unverified`, уровень «приближение»): сокращённая RLC-модель извлечения по полосам (1 кГц–`f_hi`, `f_hi` ≤ 40 МГц и ≤ предел квазистатики платы) в композиции с развязкой/VRM/нагрузками; `impedance(port)`, `transfer({from,to})`, `resonances`, `solution.zMatrix`, `bandFit`, `dcStitch`; точки вне полосы — `null`/`unavailable`, статус `partial`; время — `PI_NOT_IMPLEMENTED_STAGE` до этапа 4 |
| `networkParameters()` | да | **да** | сеть/connect/line; exact physical-port samples — только отдельный replay исходных точек и опор, без соединения |
| `si()` | да | **частично** | `.network()` — поддержанный temporal subset; `.channels([...])` — совместное P7-извлечение по спектральному запросу drivers/time, с явными пробелами и без waveform/timing |
| `tdr()` | да | **частично** | P11 v1 (`unverified`, уровень «приближение»): только standalone `network`/`connect`, один или два порта (первый — DUT, второй — дальний конец). Ступень через существующий SI-переходной путь, ρ = v/(V/2) − 1, Z = Zref(1+ρ)/(1−ρ); отсчёты до `riseS` и при |1−ρ|<1e-6 недоступны. `termination`: `matched` (rc-input R=Zref, C=1e-18 F), `open`, явная модель; `short` отклонён (`TDR_SHORT_TERMINATION_NOT_IMPLEMENTED`). `board`/`line`/`channel`/`differential`/`resource` отклоняются (`TDR_BOARD_NOT_IMPLEMENTED`, `TDR_TARGET_NOT_IMPLEMENTED`); per-port `referenceOhm` отклонён. `distance` — только при `parameters.propagationVelocityMps` на delay line, иначе недоступна. Линия с Z1≠Z0 не проверена. |
| `eye()` | да | **частично** | P11 v1 (`unverified`, уровень «приближение»): пост-обработка готового временного `signal` (например, из `si().network`); fold по UI = 1/bitRateBps после `discardS` со сдвигом `phaseS`, 64 фазовых бина на UI; height/voltageMargin в центральном окне ±0.1 UI, width/timingMargin — по спану открытости не менее 50% высоты. Без BER, протоколов, джиттера и эквализации; меньше 2 UI после `discardS` — отказ `EYE_SIGNAL_TOO_SHORT` |
| `em()` (full-wave) | да | **нет** | исключён из текущей дорожной карты |
| `emi()` | да | **нет** | — |
| `thermal()` | да | **да** | `src/analyze/analysis-dsl/thermal-executor.ts` поверх native P9: явная `network` или плата; steady/time; потери DC→thermal через `thermal-loss-transfer.ts` (сохранение мощности проверяется) |
| `electrothermal()` | да | **частично** | P10 v1 (`unverified`, уровень «приближение»): только standalone steady — `electrical.circuit` + `thermal.network` + `couplings: [{element, thermalNode}]` для резистора с `referenceC`/`temperatureCoefficientPerK`; Picard по существующим DC- и thermal-исполнителям; сходимость `coupling({toleranceK, relativeLossTolerance, maxIterations, relaxation})`; сбой — `failed` с `ELECTROTHERMAL_NOT_CONVERGED` или `ELECTROTHERMAL_RUNAWAY` и историей; мощность связи проверяется до 1e-9. Нет платы/MOSFET (`operating`), нет transient (`ELECTROTHERMAL_TRANSIENT_NOT_IMPLEMENTED`) |

## `impedance()`

| Метод / цель | В схеме | В коде | Ограничение |
|---|---|---|---|
| `method: "formula"` | да | **нет** | исполнителя нет |
| `method: "cross-section"` + `line` | да | **да** | 4 шаблона линии; полоса и потери — по режиму |
| `method: "layered"` + `channel` | да | **да** | авто-канал, 2 порта, AC 1–10 кГц; отдельный 0 Hz endpoint, без обещания непрерывного DC/AC overlap |
| `cross-section` + `channel`/`differential` | да | **нет** | отказ `SECTION_UNSUPPORTED` |
| `layered` + `line` | да | **нет** | — |
| `layered` + `differential` | да | **частично** | одна совместная четырёхпортовая модель leg-to-return; mixed-mode characteristic impedance и полная физическая приёмка открыты |

## `a.line` (явное сечение)

| Возможность | В схеме | В коде | Ограничение |
|---|---|---|---|
| microstrip, coupled-microstrip | да | **да** | — |
| stripline, coupled-stripline | да | **да** | две плоскости сводятся к общему reference (`referenceTopology: "common"`) |
| произвольный polygon-сечение | **нет** | native есть | в публичный DSL не выведен |
| конечная ширина опоры | **нет** | native есть | в `a.line` опора только по толщине |
| `lengthMm`, `delay`, `rlgc`, `network` | да | **да** | `delay` = phase delay, не group delay |
| частотные потери (C/C0, surface impedance) | да | **частично** | слабопотерный quasi-TEM, фиксированная полоса |

## `a.channel` и авто-канал

| Возможность | В схеме | В коде | Ограничение |
|---|---|---|---|
| `from` / `to` | да | **да** | одна пара контактов |
| `scope` | да | **частично** | один прямоугольник без отверстий, все медные слои |
| `via` (ограничение пути) | да | **нет** | `LOCAL_INPUT: Authored channel path constraints are not supported` |
| `viaId` как контакт | да | **да** | endpoint-via поддержан |
| несколько опор / вторая земля | через физические пары контактов | **частично** | независимые потенциалы и ориентированные связи сохраняются; общая board-level приёмка открыта |
| удалённый / распределённый возврат | через исходную медь | **частично** | совместный конечный домен, геометрический выбор foil/mixed; большой домен может исчерпать бюджет, точность unverified |
| `mesh` (`maxNodes`, `maxEdgeMm`, `domainScale`) | да | **частично** | действует только для `cross-section`; `layered` использует фиксированный профиль |
| полоса | запрос frequency/frequencyHz или спектр SI | **частично** | native local 1–100 кГц, assembled/foil 1–10 кГц; интервалы и пробелы сохраняются, HF не допущен |
| диэлектрические потери | да (материал) | **нет** | `layered` отвергает loss tangent |

## Правила (`rules`)

| Правило | В схеме | В коде |
|---|---|---|
| `connectivity`, `clearance`, `edge-clearance` | да | **да** |
| `length`, `skew` | да | **частично** (структурная проверка) |
| `reference-continuity`, `return-transition` | да | **нет** (`GEOMETRY_UNSUPPORTED_RULE`) |

## Модели и измерения

| Возможность | В коде | Ограничение |
|---|---|---|
| `a.model`: resistor / capacitor / inductor | **да** | явные паразиты или `ideal: true` |
| `linear-driver`, `rc-input` | **да** | — |
| `thermal-network`, `heatsink`, `cooling` | **частично** | только как описание, без thermal-исполнителя |
| `network` / Touchstone-подобный ресурс | **да** | проверяемый пассивный поднабор |
| `timing({ kind: "sampling" / "propagation" })` | **частично** | P11 v1 (`unverified`) только для waveform из поддержанного `si().network`: sampling — уровни в точках clock-события и setup/hold относительно порогов lowV/highV; propagation — задержка по фронту того же направления, skew (парование по порядку событий) и spread. Нет события, неоднозначное совпадение или переход вне окна дают unavailable с диагностикой, а не ноль; каналы и PCB не поддержаны |

## Что обновлять при изменении кода

1. Гейт `available` (`src/analyze/analysis-dsl/prepare.ts`).
2. Диспетчер и исполнитель (`src/analyze/analysis-dsl/executor.ts`).
3. Схему (`schema.ts`) — только если меняется публичный контракт.
4. Этот файл — статус. `dsl-reference.md` не трогать как статус.
