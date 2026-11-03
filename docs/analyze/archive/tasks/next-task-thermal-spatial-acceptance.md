# P9: закрыть пространственные проверки без повторения готовых расчётов

2026-10-24. Продолжение текущего Thermal, не новый этап. Public controller
steady прошёл; весь P9 ещё не принят. Исходные failed reports сохраняются.

## Работа

1. Исправить только доказанный сбой boundary localization в сравнении полей.
   Для 24 BandAmp points сначала подтвердить соответствие исходной boundary
   edge и transported sample. Предел displacement вывести из представления
   координат/геометрии, записать формулу и фактические смещения в отчёт.
   Нельзя произвольно расширить barycentric tolerance, заполнить holes,
   использовать nearest triangle для отсутствующей физической области или
   скрыть missing coverage. Interior interpolation не изменять.
   Отдельные negatives: реальный gap/cutout, другая boundary, точка за пределом
   ошибки координат; positives — только проверенные transported boundary points.
   Re-audit сохранённого BandAmp без нового solve: spatial10.15% останется FAIL.
2. Для TPS и BandAmp получить две пространственные сетки с полным field gate2%.
   Следующая разрешённая ordinary пара1/.5mm, те же curve tolerance/physics и
   384MiB/120s на solve. До запуска проверить allocation/output admission.
   Сначала steady; не повторять temporal cases пока steady geometry не пройдёт.
   Если pair не помещается либо не сходится, разрешена одна обоснованная opt-in
   refinement постановка с существующим policy, в тех же ресурсных пределах.
   После неё — результат или blocker, без цепочки бесконтрольных запусков.
   Финальные примеры должны остаться direct DSL, с сохранёнными fail историями.
3. Для controller transient сначала разобрать сохранённый48.23% field witness:
   время/слой/координата/температуры/норма и покрытие. Проверить начальное ambient,
   события источника, единицы и отсутствие деления на почти нулевой rise.
   Не ослаблять норму ради PASS. Дать численное объяснение большой разницы.
   Полный7-pass transient НЕ запускать. Предложить один конкретный путь уменьшить
   время/память с сохранением проверяемой пространственной/временной точности.
   Если нужен production cache/новый mesh API — только proposal, не реализация.
4. Curve.01 controller пока отложить до ясной оценки общей геометрической области
   и стоимости. Не тратить ещё один900s run, который невозможно корректно сравнить.

## Приёмка и ограничения

- Ни nodal1e-8, ни spatial2%, ни temporal0.1%, ни input/output/memory caps
  не меняются. Steady controller и пройденные package/native проверки не повторять.
- При правке comparator нужны focused regression и независимый saved-result
  negative audit; rebuild addon нужен только при изменении native sources.
- Свежие direct reports: численные поля/метрики, source identity, mesh/coverage,
  residual/energy, PNG. Показать физические различия и boundary displacement
  отдельно. Отчёт не должен выдавать sampled extrema за непрерывный максимум.
- Итог в thermal-verification.md: что прошло, что осталось, точные paths/hashes,
  команды/ресурсы. Handoff обновить в начале файла, не накапливать противоречивые
  pending instructions. Не коммитить до review. Heavy slot согласовать с P6/P7.
- Не останавливаться после простого плана: выполнить разрешённые проверки и
  исправления. Сообщать готовый пакет/существенный blocker, без частых статусов.
