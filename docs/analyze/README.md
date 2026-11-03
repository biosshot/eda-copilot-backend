# Анализаторы PCB: документация

Здесь живут планы, справочники, приёмки и архив работ по экстрактору параметров из PCB
(`src/analyze`, `native/analyze`, DSL).

Папка вынесена из `docs/experimental/analyze/` 2026-11-02 и разложена по назначению.
Старые имена файлов сохранены, относительные ссылки переписаны.

## Что читать

| Задача | Начать с |
|---|---|
| Понять общий план P0–P15 | [plans/implementation-plan.md](plans/implementation-plan.md) |
| Понять, как закрывается P7 | [plans/p7-plan.md](plans/p7-plan.md) |
| Узнать, какие случаи поддержаны | [reference/p7-extraction/p7-case-matrix.md](reference/p7-extraction/p7-case-matrix.md) |
| Продолжить текущую работу | [plans/chat-handoff-2026-11-02.md](plans/chat-handoff-2026-11-02.md) |
| Узнать, что реально работает в DSL | [reference/dsl/dsl-implementation-status.md](reference/dsl/dsl-implementation-status.md) |
| Писать сценарии DSL | [reference/dsl/dsl-reference.md](reference/dsl/dsl-reference.md) |
| Понять принятый электрический интерфейс | [reference/p5-p6-electrical/electrical-core-completion-semantics.md](reference/p5-p6-electrical/electrical-core-completion-semantics.md) |
| Проверить, что уже принято | [evidence/p6-p7-integration-acceptance.md](evidence/p6-p7-integration-acceptance.md) |

## Структура

```
docs/analyze/
  README.md                  этот индекс
  plans/                     живые планы и handoff
  reference/                 справочники и семантика
    dsl/                     контракт, справочник, матрица реализации, сценарии
    input/                   RoutingBoardV2, аудит входа
    p4-geometry/             геометрия, экспорт, валидация
    p5-p6-electrical/        электрическое ядро и частотные многопорты
    p7-extraction/           сечения, потери, HF-оператор, paging
    p9-thermal/              тепловая политика
    architecture/            численная архитектура, структура, корпус, библиотеки
    scope/                   каталог анализов, глубина, границы
  evidence/                  приёмки, результаты, ревью, контрольные точки (датированные)
  archive/tasks/             выполненные задания next-task-*
  archive/design/            черновики периода 2026-10-19…10-23, заменённые решения
  archive/handoffs/          предыдущий handoff
```

Правила:

- **Живой документ один на тему.** Остальное — датированный evidence, он не переписывается.
- `plans/` обновляется по статусу; `evidence/` — только добавляется.
- У живого документа в шапке должны быть: статус, дата обновления, что он заменяет.
- `reference/dsl/dsl-reference.md` — спецификация, а не статус. Фактическая реализация — только в
  `reference/dsl/dsl-implementation-status.md`.

## Фактический статус на 2026-11-02

Ветка `feat/analyze`, HEAD `c2fef93`. **P7 не закрыт.** P8/P11 не реализованы.

### Что принято и работает

- Импорт RoutingBoardV2, физическая геометрия меди, связность, conduction mesh.
- DC по реальной меди и явные электрические сети (P6a), частотные многопорты (P6b).
- Численные сечения линий: microstrip, coupled, stripline, coupled-stripline; C/C0/L, Z0,
  задержка, S; слабопотерный режим в подтверждённой полосе (P7a; P7b реализован, приёмка открыта).
- Локальный direct-FEM экстрактор (3D): электрика, ток, восстановление токов/потерь;
  интеграция в worker/addon/DSL и диск-кэш; полный аудит 51/51 возбуждений на исходном ROI.
- Реальные STM32/TPS PDN-куски в полосе 1–100 кГц (conditional), BandAmp RF-сечение 500 МГц–1 ГГц.

### Что открыто

- **D1 (текущий фокус).** Slot/continuous: conservative current прошёл current-gates,
  магнитная энергия стабилизирована, но последний холодный DSL упал на
  `CHANNEL_DOMAIN_CONVERGENCE` (group delay 13.7%/16.8% при пороге 2%). Незакоммиченный
  патч (direct volume-integral magnetic operator, cross-block mutuals, `electricAirMarginM`)
  собран не полностью: addon и `dist` отстают от исходников.
- **Композиция канала.** Coupon: 7.79% по задержке при 0.0006% по S; rotated45 2.17%,
  via 2.76% (порог 2%). Независимый эталон есть не для всех классов.
- **Реальный STM32-канал.** Planner отказывает на distributed lateral finite-plane interface;
  магнитный фактор 8.7 ГБ при цели 2 ГиБ.
- **Возвратные пути.** Одна опора на сечение; несколько опор, удалённый возврат и via fence
  не приняты.
- **Полоса.** Нет общей полосы section/local; частотно-зависимые коэффициенты в
  автоматическом канале отвергаются; переход skin-режима не реализован.
- **Полный P8/P11.** PI/SI/TDR/eye не реализованы.

### Что нельзя обещать по документам

- `implementation-plan.md` по тексту отстаёт от 2026-11-02:
  читать его вместе с handoff и этим индексом.
- Маршрут P8/P11 и поставка 2026-10-29 слиты в `p7-plan.md` §7.2; старый файл —
  `archive/design/p7-to-p8-p11-plan.md`.
- `dsl-reference.md` документирует и нереализованное (`method: "formula"`, `a.em`).
  Статус — в `reference/dsl/dsl-implementation-status.md`.
- `plans/chat-handoff-2026-11-02.md` и `evidence/p7-d1-magnetic-closure-results.md`
  до переноса не были в git; сейчас добавлены.

## Как добавить документ

1. План или задание — `plans/`; задание обязательно парой с будущим `*-results.md`.
2. Результат, приёмка, ревью — `evidence/`, имя с датой.
3. Справочник или спецификация — `reference/`; статус реализации туда не писать.
4. Завершённое задание и заменённый черновик — `archive/`.
5. Обновить этот README, если изменился статус или точка входа.
