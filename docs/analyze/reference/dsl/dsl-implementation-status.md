# DSL: статус реализации

Живой документ. Обновлён **2026-11-02**.
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
| `pi()` | да | **частично** | только с явным `circuit`; авто-PDN из платы нет |
| `networkParameters()` | да | **да** | сеть/connect/line |
| `si()` | да | **частично** | только с `.network()` типа network/connect; `channels([...])` без сети не подключён |
| `tdr()` | да | **нет** | — |
| `eye()` | да | **нет** | — |
| `em()` (full-wave) | да | **нет** | исключён из текущей дорожной карты |
| `emi()` | да | **нет** | — |
| `thermal()` | да | **нет** | native P9 есть, DSL-исполнителя нет |
| `electrothermal()` | да | **нет** | — |

## `impedance()`

| Метод / цель | В схеме | В коде | Ограничение |
|---|---|---|---|
| `method: "formula"` | да | **нет** | исполнителя нет |
| `method: "cross-section"` + `line` | да | **да** | 4 шаблона линии; полоса и потери — по режиму |
| `method: "layered"` + `channel` | да | **да** | авто-канал, 2 порта, полоса 1–10 кГц |
| `cross-section` + `channel`/`differential` | да | **нет** | отказ `SECTION_UNSUPPORTED` |
| `layered` + `line` | да | **нет** | — |
| `target: differential` | да | **нет** | только как два отдельных канала |

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
| несколько опор / вторая земля | **нет** | **нет** | одна опора на сечение |
| удалённый возврат | **нет** | **нет** | нужен joint-домен или nonlocal-модель |
| `mesh` (`maxNodes`, `maxEdgeMm`, `domainScale`) | да | **частично** | действует только для `cross-section`; `layered` использует фиксированный профиль |
| полоса | — | **фиксирована** | материал должен покрывать 1–100 кГц |
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
| `timing({ kind: "sampling" / "propagation" })` | **да** | для `si` с поставленной сетью |

## Что обновлять при изменении кода

1. Гейт `available` (`src/analyze/analysis-dsl/prepare.ts`).
2. Диспетчер и исполнитель (`src/analyze/analysis-dsl/executor.ts`).
3. Схему (`schema.ts`) — только если меняется публичный контракт.
4. Этот файл — статус. `dsl-reference.md` не трогать как статус.
