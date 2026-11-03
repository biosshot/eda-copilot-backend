# Справочники и семантика

Здесь лежит то, что описывает **как устроено и как пользоваться**, а не что уже принято.
Приёмки и результаты — в `../evidence/`, планы — в `../plans/`.

## Разделы

| Папка | Что внутри | С чего начать |
|---|---|---|
| [dsl/](dsl/) | Публичный DSL: контракт, полный справочник, матрица данных, статус реализации, сценарии | [dsl-implementation-status](dsl/dsl-implementation-status.md), [dsl-reference](dsl/dsl-reference.md) |
| [input/](input/) | RoutingBoardV2: форма, достаточность, аудит входа, контракт платы | [routing-board-v2-shape](input/routing-board-v2-shape.md) |
| [p4-geometry/](p4-geometry/) | Экспорт геометрии, валидация, единицы и допуски | [geometry-validation-semantics](p4-geometry/geometry-validation-semantics.md) |
| [p5-p6-electrical/](p5-p6-electrical/) | Электрическое ядро, сети, частотные многопорты | [electrical-network-semantics](p5-p6-electrical/electrical-network-semantics.md) |
| [p7-extraction/](p7-extraction/) | Сечения линий, потери, HF-оператор, OS paging, матрица случаев P7 | [p7-case-matrix](p7-extraction/p7-case-matrix.md), [line-cross-sections-semantics](p7-extraction/line-cross-sections-semantics.md) |
| [p9-thermal/](p9-thermal/) | Политика тепловой сходимости | [thermal-convergence-policy](p9-thermal/thermal-convergence-policy.md) |
| [architecture/](architecture/) | Численная основа, структура реализации, корпус, библиотеки | [numerical-architecture](architecture/numerical-architecture.md) |
| [scope/](scope/) | Каталог анализов, варианты глубины, границы, обзор инструментов | [working-analysis-scope](scope/working-analysis-scope.md) |

## Правила

- Спецификация и статус — разные документы. Статус DSL живёт только в
  [dsl-implementation-status](dsl/dsl-implementation-status.md); `dsl-reference` не переписывается
  под текущее состояние кода.
- Новый справочник кладётся в раздел своего этапа; если этап не назван — в `architecture/`.
- Приёмка, отчёт, ревью и датированный результат в `reference/` не кладутся.
- Матрица случаев — статусный справочник: она обновляется вместе с evidence, но остаётся здесь,
  потому что отвечает на вопрос «что поддержано», а не «что произошло в прогоне».
- Ссылки на файлы этапов (`P8`, `P11`) отсюда не ведутся, пока этап не начат.
