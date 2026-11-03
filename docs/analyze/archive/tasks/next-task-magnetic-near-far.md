# P7: near/far стоимость и магнитная ошибка

Исполнитель p7_medium. Magnetic kernel принят только на frozen primitives/closed-current coupon; dense ROI по-прежнему слишком дорог. Продолжить в прежнем worktree без переделки C/P6/Thermal.

## Задача

1. Переиспользовать существующий far.rs: построить bounded spatial hierarchy/план near/far взаимодействий для трёх сохранённых ROI. На этом этапе считать размеры, counts, upper storage и стоимость по измеренным kernel timings, не запускать full dense ROI.
2. На небольшом замороженном closed-current купоне реализовать изолированный near-exact/far-moment оператор с явным remainder контролем. Все self/near взаимодействия — принятый kernel, геометрия не меняется. Никакого отсечения малых членов по произвольной величине и подмены L=inverse(C).
3. Свести pair/block bounds в оценку полной квадратичной энергии на физическом пространстве токов. Показать, где это строгая оценка truncation, где empirical quadrature/reference uncertainty. Нулевая/почти нулевая энергия, cancellation и слабые собственные моды требуют отдельного absolute bound; per-pair relative gate недостаточен.
4. Проверить полный оператор против сохранённого dense/reference на купоне, все self/mutual, reciprocity/PSD и поле действия на произвольный вектор. Не лечить отрицательные собственные значения clipping. Для acceptance сохранить исходный full energy gate <=5% с учётом reference uncertainty, near/far бюджет зафиксировать до измерения.

## Выход и границы

Таблица трёх ROI: near/far counts, hierarchy construction/storage, ориентировочная стоимость и причины возможного отказа. Малый native prototype + независимые tests по удалению/сближению, rotation/scale, mixed slender geometry, нарушению separation и budgets. Проверить monotonic error при refinement и консервативность bound на полном coupon. Frozen reference не перезаписывать, численные error/eigenvalue/cost plots и hashes сохранить.

Не подключать публичную PCB экстракцию/vias/DSL, не делать large ROI solve или reduction. Если aggregate bound слишком груб или экономия мала — конкретный blocker с данными, без ложной приёмки. Coupons <=512MiB/60s; preflight до allocation, без повышения лимитов и повторов неизменённых зелёных расчётов. Native jobs1/heavy slot согласовать. Без коммита/новых агентов, отчёт и стоп.
