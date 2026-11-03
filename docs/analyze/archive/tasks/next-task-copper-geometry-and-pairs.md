# P7: единые координаты меди и ускорение классификации пар

## Решение и результат

Пользователь согласовал общий координатный контекст и потребовал включить ускорение в это же задание. Эта редакция заменяет прежнюю остановку на proposal об общем представлении. Нужна реализация обоих изменений с измерением скорости, а не ещё один отчёт о вариантах.

Ускорение: индивидуальные существующие moment separation/remainder checks внутри coarse near leaves. На купоне подход подтверждён; аудит3ROI выявил48–62% потенциально переклассифицируемых near pairs. Оценка1.9–2.7x относится к условной работе, НЕ к гарантированному времени всего P7. Считать и проверять итоговый aggregate error обязательно.

## A. Общий координатный контекст

- Использовать уже установленную геометрическую библиотеку и её integer/explicit-adapter API. Один детерминированный origin/scale для полного контекста подготовки платы, с запасом на габариты примитивов/операций. Источники, layers/nets сохраняют идентичность, но получают согласованное преобразование координат. Не выбирать новый adapter для каждой дорожки или промежуточного результата.
- Согласованно провести primitive assembly, union, board clipping и drill subtraction. Не терять эту согласованность при последующем layer union/переходе к mesh. Указать, какие операции относятся к production Rust, а какие пока к experimental extraction harness; исправление только Python-копии не считать исправлением production.
- По возможности удерживать integer coordinates между операциями, конвертируя в mm только на границах. Общие authored endpoints/cap facets преобразуются одинаково; intersections получают детерминированную обработку. Не строить собственный exact-rational engine, новую универсальную геометрическую подсистему или RoutingBoardV3.
- Исполнитель сам выбирает минимальный internal API и численные параметры на основе диапазона integer API и существующего geometry/Boolean error budget; отдельное согласование каждого поля не требуется. Документировать overflow/extent и накопление погрешности. Если согласованное представление не укладывается в существующий бюджет, вернуть конкретный blocker; новый пользовательский snap tolerance не вводить.
- Никакого удаления по площади/aspect ratio, склеивания разных сетей или нулевого вклада вместо failed integral. Для неразрешимых контактов использовать существующую диагностику/unsupported; не проектировать сложную новую систему авторемонта.

Основной критерий: устранить воспроизведённые округлительные артефакты STM668/556/613 без потери настоящего0.2µm bridge из counterexample. Восстановление источников уже выполнено — не повторять исследование с нуля. Изменить только необходимые helpers подготовки меди и их потребителей.

## B. Ускорение в том же изменении

- В coarse near block проверить каждую пару существующим individual-ball moment admission. Self и непрошедшие условия остаются exact-near. Успешные пары переходят в far с тем же контролем remainder; для каждой пары ровно один путь вычисления.
- Повторно использовать вычисленные centers/radii/moments и сохранять classification в ограниченном плане, чтобы не повторять expensive predicates/подготовку при каждом action. Не раздувать plan сверх действующего бюджета; допустимо compact representation/streaming там, где оно дешевле.
- Не увеличивать theta, не ослаблять far error budget, не добавлять FMM/GPU/новую форму current basis. Важен меньший объём exact-near работы, а не полная переделка far solver.
- Сохранять bound для полного физического пространства токов, zero/weak modes, reciprocity/PSD и честные отказы. Геометрия и классификация могут разрабатываться в одном задании, но измеряются отдельно: A-baseline и A+B имеют одну и ту же исправленную геометрию.

## Проверки правильности

1. Минимальные source→union→mesh regressions STM613,668/556; true bridge/gap, разные сети, одинаковые endpoints, повороты/polyline, разная ширина, translation/rotation, pads/pours, drills/cutouts. Корректность контактной топологии сравнивать с authored intent и исходными объектами, не объявлять старый артефакт обязательным golden output.
2. Geometry/connectivity comparison всего доступного корпуса8 frozen boards с source IDs/доказательствами differences. Не выполнять live re-export. Малые conduction/DC regressions обязательны, если меняются общие helpers; placement F32/GPU не менять.
3. Проблемные post-fix supports должны проходить нужные finite-prism проверки без unchecked fallback. Independent magnetic coupons, full energy gate<=5% с отдельной reference uncertainty; near integral gate0.2%, прежние reciprocity и aggregate far budget не ослаблять.
4. Pair coverage без потерь/дубликатов; forced-all-near vs optimized на малых coupons, null/weak modes и arbitrary-vector actions. Нельзя считать скорость на отказавшем baseline успешным ускорением.

## Измерение скорости — обязательная часть

До запуска зафиксировать конфигурации и результаты, которые сравниваются:

- G0: старая подготовка геометрии (валидные случаи; failed случаи отдельно).
- G1: единые координаты, прежняя coarse near classification.
- G2: та же исправленная геометрия + individual near checks.

На8boards измерить geometry preparation отдельно: cold/warm, wall, planned/observed memory, counts. На3ROI — plan construction, near/far counts, сохранённые moments/bytes. На малом физическом купоне — полный extraction/action wall вместе с классификацией, exact-near и aggregate-bound работой, а не только число пар.

Не начинать с заведомо многочасового fullROI. Использовать representative deterministic near/far batch и count-only real plans; отдельно назвать extrapolation. Максимум два новых numerical coupon batches <=512MiB/120s каждый (G1/G2); короткие timing replays максимум3 на конфигурацию, без повторного независимого reference solve. Фикстуры/пороги/выборка заданы до результата.

Acceptance ускорения: реальное уменьшение exact-near work и measurable end-to-end gain на объявленном валидном купоне при тех же error gates; geometry preparation и общий memory peak не должны существенно регрессировать без объяснённого tradeoff. Записать исходные timings/variance, не выдавать шум или снижение точности за выигрыш. Если быстрая часть работает, но baseline/новая геометрия какого-то ROI невалидна — обозначить её отдельно, не скрывать целиком полезный результат и не объявлять весь P7 готовым.

## Границы поставки

Один review packet с двумя логическими частями (geometry / classification), явными поддержанными случаями и blockers. Numeric/source overlays, topology differences, pair-count/cost/error plots и hashes. Tests используют сохранённые fixtures; артефакты в ignored debugging.

CPU F64 анализ, прежние caps, jobs1/isolated target. Если production Rust изменён — один normal shared-addon build после focused tests; проверить compile/package совместимость, не менять ABI без необходимости. Общие файлы согласовать с Thermal, чужие изменения не переносить. Не начинать P8/P11/fullROI/CFD, не создавать агента/сервис/общую compute библиотеку. Без коммита исполнителя; итоговый отчёт и STOP для review.
