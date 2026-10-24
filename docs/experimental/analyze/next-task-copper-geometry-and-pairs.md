# P7: согласованная геометрия, затем классификация пар

Исполнитель p7_medium, прежний worktree. Основание copper-partition-audit.md. Две последовательные фазы; failure первой не обходить ускорением второй. CPU F64 для анализа; placement F32/GPU и транспортный RoutingBoardV2 контракт не менять.

## Фаза A — исправление геометрии

1. Заморозить минимальные regression inputs из source IDs STM668/556 и613. Полностью проследить происхождение613: предыдущий аудит не восстановил всю цепочку его возникновения. Установить, какие операции в native source polygons и экспериментальном GEOS layer union реально создают артефакт; не переносить исправление Python prototype в production под видом готовой PCB extraction.
2. Реализовать минимальный способ согласованной геометрии общих авторских соединений: одинаковая точка/ширина/контакт не должны независимо округляться в разные искусственные границы. Выбрать механизм по доказанному происхождению; не назначать произвольный snap epsilon, фильтр площади или удаление всех тонких triangles.
3. Сохранять реальные nets, gaps, bridges, drills, holes, pads, pours, material/layer interfaces и port/source IDs. Описать величину геометрического изменения и её связь с существующим approximation/Boolean error contract. Если исправление требует нового допуска/изменения контрактов, остановиться с конкретным предложением; не вводить его молча.
4. Проверить не только исчезновение sliver, но источник-полигон→union→mesh→finite-prism chain. Новый near self интеграл проблемного участка должен проходить проверяемую accuracy/refinement либо явно оставаться unsupported. Нельзя заменить бывший error нулевым вкладом.

Приёмка A: реальные near-touching разные сети не замкнуты; настоящая тонкая перемычка не исчезает; общие узлы/повороты/разная ширина, translate/rotate, boundaries/holes, microscopic artifact regressions. Геометрия и connectivity на ВСЕХ доступных frozen board fixtures с отчётом differences/source IDs, без нового live export. На3ROI — density/contour/support counts before/after. Независимые magnetic coupons/energy gates сохранить. Большие DC/полные magnetic ROI не запускать. Если изменяется общая подготовка меди — обязательно focused geometry/connectivity/conduction/DC regression на малых случаях, не только тесты P7.

## Фаза B — индивидуальная классификация внутри coarse near leaves

Разрешена лишь после выполнения критериев A. Отдельным логическим diff внедрить existing individual moment separation/remainder checks для пар внутри coarse near blocks. Self и непрошедшие checks остаются exact-near. Геометрию/current basis дополнительно не менять; shared vertex/edge pairs не объявлять far без критерия. Никакого unchecked fallback.

Проверить полноту/единственность пар, reciprocity/PSD, нулевые/слабые режимы, полную aggregate physical energy bound и сохранённый independent full-operator reference. Per-pair error недостаточно. Сравнивать baseline и candidate на одной геометрии ПОСЛЕ фазыA; не смешивать изменение mesh и классификации в одном объяснении speedup.

Count-only plans на3ROI: сколько near осталось, bounds/storage/preflight. Не объявлять1.9–2.7x гарантированным результатом, не запускать full ROI. Максимум один bounded coupon batch <=512MiB/120s на фазу; expensive source changes оправдывают focused rerun, green unchanged tests не повторять. Runtime caps не повышать. Общий addon rebuild лишь если изменён production path, согласовать heavy slot с Thermal, jobs1/isolated target.

Результат: два раздела implementation/evidence, чёткая граница исправленного production и experimental prototype, source overlays, topology comparisons, matrix/error/cost PNG, hashes и оставшиеся blockers. Сохранить все старые failure artifacts. Без коммита/новых агентов. После отчёта STOP, P7/P8/P11 целиком не закрывать.
