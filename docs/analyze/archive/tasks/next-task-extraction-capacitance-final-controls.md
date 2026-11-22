# P7: финальные раздельные controls для текущего capacitance coupon

2026-10-24. D32 combined full/common5.5964/5.5963% FAIL; differential3.5219%
PASS. Не поднимать accuracy tolerance5% и не объявлять production готовым.

Разрешены не более ДВУХ новых solves с reuse всех предыдущих outputs:

1. Domain-only D64 на сохранённом D32 combined FE space: внутренние функции,
   constraints, foil/gap и physical potentials неизменны. Добавляется только
   внешний domain. Проверить exact nesting до numeric solve; получить отдельный
   domain change и независимые full/common/differential errors.
2. Только если сохранённые flux indicators обосновывают конкретный недостаток
   пространственного разрешения, один nested mesh-only control на том же D64.
   Выбор edges/planes и ожидаемый эффект записать до solve. Не равномерное
   refinement ради достижения green и не новый solver/mesher.

Диагностический cap для этих controls явно расширен до50000nodes/6millionterms;
это НЕ изменение production API/defaults. Прежние512MiB/60s на solve остаются,
actual symbolic/полный simultaneous memory plan обязательны. Если не помещается,
остановиться с конкретным allocation evidence, без скрытого повышения лимитов.

Проверять independent full energy error<=5%; domain/mesh changes показывать
отдельно, целевой last-pair<=2% каждый. Даже малое изменение или один coupon PASS
не доказывают общую пригодность для arbitrary PCB. Сохранить all failed history,
raw unsymmetrized reference, source hashes, scalar/mode errors и один PNG.

После controls: готовый численный пакет и следующий implementation proposal
либо concrete blocker. Если C gates проходят — минимальный план переноса
проверенной постановки в Rust с reuse существующего CDT, список новых accuracy
coupons для dielectric/layers/vias и реальные limits. Не запускать этот перенос
без review. L kernel conditioning/cost и full quadratic magnetic gate остаются
отдельными обязательствами, не пропускать их. Коммитов/новых агентов не делать.
