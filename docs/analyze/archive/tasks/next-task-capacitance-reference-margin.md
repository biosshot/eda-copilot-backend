# P7: устранить неопределённость three-conductor reference

2026-10-24. Native port/evidence рассмотрены как незавершённый пакет.
Exact parity4.92e-14 и raw three-conductor4.90099% не заменяют достаточный
запас относительно reference change0.17802%. Production пока не принят.

1. Переиспользовать сохранённые native matrices, не пересчитывать их без причин.
   Получить один более точный offline FastCap reference для той же three-foil
   геометрии. Выбрать refinement после memory/time preflight и зафиксировать
   до запуска. Разрешён explicit offline budget1024MiB/60s, только reference;
   production/native coupon512MiB/60s остаётся прежним. Не runtime dependency.
2. Пересчитать полную positive-range ошибку native C против нового reference,
   scalar raw entries и изменения всех трёх последовательных reference уровней.
   Не называть observed change доказанной continuum error bound. Для рабочего
   acceptance screen требовать native error +2*последнее reference change<=5%,
   явное убывание reference changes и отдельное сохранение этих величин.
3. Если запас всё ещё отсутствует, разрешён один обоснованный nested native
   mesh-only control по прежнему geometry/domain и actual symbolic admission:
   explicit diagnostic60000nodes/7Mterms/768MiB/60s. Это ограниченное расширение
   только coupon studies, не defaults. Перед solve записать selected region,
   expected effect и inherited constraints. Не менять физику/reference input.
4. Итог — raw matrices/hashes/uncertainty table, независимый audit/PNG,
   source provenance и точные residual/recovery/resource gates. Если всё ещё
   FAIL, остановиться с доказательством; новых серий tuning не начинать.

Не начинать vias/PCB/DSL/L/reduction. Не повторять green two-foil studies.
Доработки native kernel допустимы только для конкретного найденного дефекта,
с соответствующим regression. После результата обновить §8.17/handoff,
выдать пакет для code review и остановиться. Не коммитить до parent review.
