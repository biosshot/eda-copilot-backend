# P9: native boundary correspondence и телеметрия сходимости

2026-10-24. TPS policy steady проходит0.8812%; BandAmp остаётся blocked24points
в native comparator. TS исправление само по себе не закрывает native policy.
P9 не принят, исходные failed reports и пройденные checkpoints сохраняются.

## Реализовать

1. Перенести доказанное правило boundary correspondence в native thermal
   refinement comparator. Использовать исходную физическую границу и однозначную
   циклическую связь rings/edges; сохранить ту же edge fraction и явный bound
   displacement. Не nearest-triangle fallback и не увеличение barycentric epsilon.
   Не переносить точки holes/реальных gaps на внешний ring. Если источника
   соответствия не хватает — адресная missing coverage, а не догадка.
2. В scalar comparison history сохранить ДВА witness: абсолютного максимума
   и максимума нормированной ошибки, каждый с time/layer/coordinate, coarse/fine
   temperatures, ambient и denominator. Не сохранять все предыдущие поля.
   Это внутренняя result telemetry, без нового пользовательского DSL.
3. Focused native positive/negative fixtures для transported boundary, holes,
   removed cell, wrong source identity, over-bound displacement, interior point.
   TS/native должны одинаково классифицировать один frozen fixture. Проверить
   scale/offset и отсутствие изменения nodal/energy/spatial/time tolerances.
4. Штатный addon build jobs1/default GPU и один BandAmp direct policy rerun
   с прежними50k/384MiB/120s/128MiB. Требуется independent control, missing0,
   full field<=2%, original KCL/energy, negative audit и просмотр PNG.
   Если120s недостаточно, показать phase costs; не давать каждому pass новый budget.

## Controller transient — только bounded optimisation checkpoint

Оценить предложенное reuse одного SPD preconditioner между разными dt на
небольшом faithful thermal coupon. Разрешён один A/B эксперимент с теми же
физическими операторами/источниками и критерием residual, не fullboard.
Записать до запуска правило refresh при stagnation и admission единственного
factor/cache slot. Сравнить температурные поля, local time error, nodal/energy,
iterations/factorizations/wall/memory. Не выдавать ускорение малого coupon за
готовность7-pass controller. Изменение production cache policy — только после
review результата; полный controller transient и curve.01 пока не запускать.

## Завершение

Обновить verification/handoff/source manifest. Не повторять дорогие green
controller/TPS solves, если исправление не затрагивает их физический расчёт;
focused native/comparator/package smoke достаточно для изменённого пути.
Сохранить исходные failed data. Коммит только после parent review всего пакета.
Согласовывать heavy слот с P7; новых этапов/агентов не запускать.
