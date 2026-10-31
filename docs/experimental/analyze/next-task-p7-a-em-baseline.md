# P7-A: EM-сценарии, независимые эталоны и исходные измерения

2026-10-31. Реализация этого задания разрешена. Родительский план: [единый P7](p7-real-channel-closure-plan.md), этап A. Это разработка воспроизводимой EM-приёмки, не новый solver и не закрытие P7.

## Результат

Один machine-readable manifest, исполняемый runner, прямые DSL-сценарии и отчёт, позволяющие сравнивать следующие изменения на неизменных физических задачах. Обязательно выполнить хотя бы один настоящий coupled AC/EM контроль существующим численным ядром и независимое сравнение. Проверка нескольких заданных резисторов не выполняет это требование.

## Перед началом

Прочитать AGENTS.md, CHANGELOG, родительский план, p7-route-partition-results.md, p7-memory-architecture-audit.md, p7-shared-return-*-results.md. Найти существующие helpers/scripts/references до добавления новых. Последние8 DC graph passes и24 unavailable EM — НЕ baseline точности электромагнитного извлечения.

## Объём и файлы

Владение: tests/fixtures/analysis-p7-closure/, tests/helpers/p7-closure-*.ts, tests/analyze-p7-closure-*.test.ts, scripts/verify-p7-closure-*.ts (или Python-supervisor только при недостаточности существующего), docs/experimental/analyze/p7-a-em-baseline-results.md. Сценарии разрешено хранить в новой fixture-папке с ссылкой на исходный board; это обычный прямой DSL, не SavedScenario. Не менять production src/native, зависимости, старые платы/сценарии/эталоны, общий roadmap/CHANGELOG. Изменения существующего общего helper согласовать с родителем; сначала переиспользовать его.

Сосед /root/p7_b_geometry исправляет native геометрию и владеет сборками. Согласовать бинарник/хеш и окно numerical runs сообщением. Не запускать solver во время замены addon/build. Не ждать без работы: сначала manifests, источники, сценарии, harness. Не создавать субагентов, не коммитить/пушить.

## Что выполнить

1. Manifest физических случаев и наблюдаемых: обычные straight/coupled sections; rotated/bend/via; две линии над split reference; несколько опор; AGND/DGND copper и component bridges; regular/broken via fence; PDN. Указать geometry/material/contact/port/load conventions, частотную ось и источник эталона. Cases, которых ещё нет, создать как полные физические coupons; не подменять произвольными RLC.
2. Реальные платы: STM32G0_CAN_RS485 U5.7/U5.5→J5.2/J5.3; NanoScope4layer; ThunderF7226layer; TPS PDN; BandAmp RF либо обоснованная замена из корпуса. Проверить экспорт и выбрать конкретные физически связанные endpoints/returns, ветви/слои, цели анализа. Для NanoScope и Thunder выбрать ПОЛНЫЕ каналы сейчас, не оставить «endpoints unselected». Сохранить exact-byte hashes и прямые runnable DSL. Не выдумывать datasheet/material data: явно заданный benchmark stack является допущением, а не измеренной платой. Отсутствие данных отмечать на уровне конкретного поля.
3. Источники: первичные аналитические/научные/измеренные references с достаточными данными; существующие whole-volume результаты — отдельный numerical reference, goldens — отдельная regression category. Пригодные сохранённые results переиспользовать с hash/units/ports. Если PDF text corrupt — посмотреть страницы/рисунки, не объявлять данные отсутствующими только по плохому OCR; если всё равно недостаточно, так и записать. Не устанавливать внешний production solver.
4. До результатов назначить метрики: complex port response, delay/Z, off-diagonal coupling, total/regional loss/return recovery где доступно. Основной ориентир8%, явно approximate15% только при независимой проверке. Absolute floors выбрать обоснованно и заранее. Не выбирать floor после неудачного сравнения. Отдельно numerical residual, sensitivity и uncertainty reference.
5. Реально исполнить доступный малый coupled section AC контроль: извлечение C/L (и потерь при поддержке), распространение по ненулевой длине, ненулевая взаимная связь на нескольких положительных частотах и независимое аналитическое/опубликованное/ранее подтверждённое comparison. Проверка той же формулой, вызывающей код производителя, не независима. Дополнить сохранёнными straight/bend full-channel данными с чётким обозначением «reused», а не «new run». Если selected control блокируется, искать существующий поддержанный EM control, а не заменять его DC. Не расширять production solver для этой задачи.
6. Runner различает pass/fail/unsupported/blocked/missing-reference; обязательный unavailable не успех. Не терять diagnostic под словом pending. Проверить отрицательными tests единицы/порты/хеши/полосу/матрицы/порог ошибки, а не только synthetic reference-fed pass.
7. Измерять cold end-to-end, OS process-tree peak private/RSS, time-by-stage при наличии; номера входов/threads/cache/build. Не мерить отказ как успешный solve и не сравнивать tiny example с whole STM32. Слишком короткие sampled peaks помечать lower bound. Для пока невозможных real cases сохранять известный blocker вместо повторного дорогостоящего отказа.
8. Графики: геометрия реальных endpoints/ветвей/опор, AC response и ошибки, coupled transfer, resource table. Просмотреть изображения. Никакой подписи «EM geometry» у схемы резисторов.

## Бюджеты и приёмка

Одна тяжёлая задача одновременно, по согласованию с native-агентом. Диагностический максимум одной попытки8GiB/600s с предварительной проверкой host headroom; меньше для малых controls. Не повышать лимит и не повторять неизменный failure. Для реальной рабочей цели сохранить300s/2GiB STM32,300s/4GiB сложный case как цели родительского плана, не повод фальсифицировать результаты. Числа частот ограничены, не каждый герц.

Приёмка A: runnable manifest/runner; конкретные полные real channels; минимум один положительный настоящей AC/EM coupled численный результат и независимая проверка; сохранённые references прослеживаемы; failed/unavailable видны; meaningful tests и inspected plots; точные команды и hashes. Будущие split/fence/HF cases могут остаться blocked, но это не закрывает их P7 gates. Если настоящий EM контроль не выполнен — задача A неполна, это явно написать.

Не принимать новые формулировки и не объявлять P8/P11 ready. Отчёт должен дать следующие этапы проверяемыми входами, а не очередной общей рекомендацией.

## Продолжение после ревью: независимая EM-точность

Авторизовано продолжение 2026-10-31. B1 геометрия прошла родительское ревью в узких границах; не менять native/production. A остаётся незавершённым. Новый владелец продолжения: `/root/p7_a_em_accuracy`. Параллельный `/root/p7_a_dsl_validation` отвечает за direct-DSL/status regression по next-task-p7-a-dsl-validation.md.

Три замечания ревью: homogeneous TEM delay не проверяет независимо абсолютные C/L/Z/crosstalk; finite35um copper сравнивался с thin reference без оценки systematic discrepancy; положительный stripline вызывал native напрямую, а его comparator fail не давал nonzero exit. Последние два workflow-вопроса исправляет сосед; ты отвечаешь за физическую сопоставимость и остаток A.

Твои файлы: существующие verify-p7-closure-baseline.ts, verify-p7-closure-fixtures.py, verify-p7-closure-plots.py, tests/fixtures/analysis-p7-closure/manifest.json и физические coupons/references/DSL кроме coupled-stripline.js; новые tests/analyze-p7-closure-accuracy.test.ts и docs/experimental/analyze/p7-a-em-accuracy-results.md. Старые отчёты не переписывать: дать новый итог, отличающий reused от new. Не менять общий helper p7-closure-reference.ts: это владение соседа, изменения согласовать сообщением. Поля supplementalControls в manifest сохранить; stripline hash обновлять только по handoff соседа.

Обязательная приёмка продолжения:

1. Получить положительный настоящий coupled AC расчёт существующим кодом и независимое сравнение абсолютных C/L/Z и взаимного отклика в выбранной полосе. Reference должен соответствовать физике, включая толщину/порты/материалы, либо иметь обоснованную ограниченную discrepancy, учитываемую в общем8% допуске. Нельзя просто принять35um-vs-zero-thickness с unbounded uncertainty. Сначала изучить уже существующие численные tests/references и физически подходящий малый случай; не повторять прежние четыре отказа с теми же входами.
2. Не расширять solver или ослаблять его convergence/memory guards. Если конкретный microstrip слишком дорог, допустим другой небольшой связанный физический case с независимым reference; сохранять целевой ненулевой mutual effect и факт извлечения из геометрии. TEM identity-only, resistor graph, producer-generated expected values не засчитываются.
3. Завершить минимальные настоящие geometry/material/contact coupons для split/multiple-reference/AGND-DGND bridge/regular-and-broken-fence и machine-readable покрытие. Будущие unsupported случаи можно явно обозначить, но нельзя оставить только названия вместо входов или объявить их успешными. Не требуется реализовать будущие C/D/F.
4. Проверить физическую связность выбранных полных real channels/returns существующей geometry API; показать землю/ветви/контакты на графиках, не только треки выбранного net name. Недостоверные endpoints исправлять в новых benchmark inputs с новым hash и объяснением, исходные платы не менять.
5. Общий runner не выдаёт pass для отсутствующего reference/непроверенной применимости/несопоставимой физики. Разделить comparisonPassed, physicalValidation и runnable status либо эквивалентную однозначную структуру. Задать допуски до результатов, собрать error/response plots и просмотреть. Добавить meaningful negative controls именно для mismatch/unknown uncertainty.

Один heavy run одновременно с соседом; сообщениями передавать окно. Стабильный addon F0CD8D66A2A190EE32918CC8B73C7669B35A5E9B568640713D9AFC166258CC85 проверить перед работой, не пересобирать. Существующие разумные budgets не повышать автоматически; общий разрешённый потолок диагностической попытки8GiB/600s не повод увеличивать предыдущую задачу без конкретного resource plan. Измерять весь DSL-процесс, не reuse-only comparison. После результатов остановиться, без коммитов и новых этапов. Если обязательная independent EM приёмка снова не получена, A остаётся незавершённым; никаких подмен.
