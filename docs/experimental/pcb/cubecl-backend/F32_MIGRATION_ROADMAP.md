# Переход вычислительного PCB backend на F32

Дата закрытия: 2026-10-01. Ветка: `feat/pcb-f32-migration`.
Статус: **ЗАКРЫТ по указанию пользователя, в согласованном после очистки объёме**.

## Итог закрытия

- [x] Перенос вычислительных путей CPU/GPU на F32 без production legacy F64.
- [x] Удаление добавленной системы интервальной физической геометрии;
  сохранение прежних алгоритмов и согласованного адаптера координат.
- [x] Сборка единого `.node`, Rust CPU/GPU-feature tests, TypeScript и
  целевые проверки настоящего GPU. Результаты — в
  [отчёте](F32_MIGRATION_RESULTS.md#cleanup-verification).
- [x] Сохранение исторических измерений и явное указание их ограничений.
- [x] Закрытие roadmap и фиксация реализации в Git.

Оставшиеся ограничения не объявлены исправленными: границы координатного
адаптера и записанное в отчёте ухудшение маршрутной метрики ESPower остаются
для отдельного обсуждения. Полная первоначальная матрица измерений не выдается
за выполненную; неподтверждённый выигрыш 2× не заявляется. Новые тяжёлые прогоны
для закрытия не требуются. Перечень ниже сохранён как исторический план,
а его незаполненные пункты не являются активными заданиями этого roadmap.


**Уточнение объёма после пользовательского пересмотра:** миграция сохраняет прежние
алгоритмы на F32. Отменено расширение задачи до новой интервальной физической
геометрии; соответствующая реализация удалена. Прежние отметки checklist ниже —
история проверок, а не основание снова добавлять снятые требования. Фактически
выполненные проверки и ограничения приведены в `F32_MIGRATION_RESULTS.md`.

## Решение и границы

Перевести все вещественные вычисления PCB solver на F32: CPU generator,
CPU scoring/search/geometry, Micro-A*, block solver, board packer, local/final
refiner, fast route, passive island и все соответствующие CubeCL kernels.
Малые CPU модули тоже переходят на F32, но это не означает перенос их на GPU.
Сохранить общий runtime, bounded batches, ограничения CPU workers, один `.node`
и отсутствие новых runtime/compiler/helper зависимостей. Ориентир размера —
около 20 МБ. Целочисленные IDs, индексы, углы, counts, grid cells остаются integer.

**Не оставлять legacy F64:** ни второго production backend, ни флага precision,
ни скрытого F64 accumulator/CPU fallback, ни копии старого solver в новой ветке.
Ошибка GPU повторяет исходный этап на **CPU F32**. F64 solver доступен только
через старую Git revision и отдельный прежний addon для внешней проверки.
Узкое исключение для binary64 арифметики границы — вычитание origin **до**
narrowing и восстановление абсолютного output (раздел 2.1). Это адаптер
координат, не F64 scorer, validator или поисковый fallback.

Пользователь допускает другие scores, порядок кандидатов, траекторию поиска и
итоговые poses, если геометрия корректна и практическое качество сравнимо либо
лучше. Разницу в последних знаках и равноценные перестановки не исправлять ради
совпадения со старой веткой. Состав обязательных стадий, hard constraints,
маршрутные обязательства и бюджеты поиска не сокращать ради ускорения.
Гипотеза порядка 2× основана на пользовательских microbenchmarks; полный solver
и память процесса измеряются отдельно, множитель не является обещанием.

Это roadmap PCB вычислений. Несвязанный schematic layout, UI, timestamps,
отчёты и JSON транспорта не переводятся механически. JavaScript `number`
остаётся binary64 по устройству языка: это допустимая оболочка API, а не
разрешение оставить численный PCB solver на F64. TS операции, влияющие на
candidate generation, geometry, score и выбор, должны получить согласованную
F32 семантику либо использовать общий native helper. Только запись результата
цепочки double-операций в Float32Array не делает всю цепочку F32.

## 1. Ветка, baseline и правила сравнения

- [x] До изменения кода создать новую рабочую ветку от актуального завершённого
  Refiner. Зафиксированный F64 anchor:
  `51274d56778cb03f8b3fda18c5a22e0ecdc6bd64`, ветка `feat/block-solver-cubecl`.
  Реализация: `feat/pcb-f32-migration`, fork совпадает с указанным anchor.
  Отдельного doc commit пока нет.
- [x] Сохранить вне production tree manifests старого addon, input/source hashes,
  release flags, hardware/driver, threads и backend settings. Использовать
  сохранённые CPU/GPU captures предыдущих задач. Не брать старую сборку только
  по имени ветки: она подвижна, опора — commit и SHA-256 бинарника.
- [ ] Старую F64 сборку запускать отдельным процессом из изолированного каталога
  проверки/checkout, не перезаписывая F32 addon. Не добавлять F64 Cargo feature,
  зависимость или дублирующую реализацию в новую ветку.
- [x] Прочитать текущие AGENTS, GPU API, source map, завершённые roadmap/results.
  Для этой задачи новые решения пользователя о ветке, F32 CPU и приёмке по
  геометрии заменяют исторические требования «работать только в старой ветке»,
  «CPU остаётся F64» и «совпадать точно с F64». Исторические отчёты не переписывать
  так, будто они измеряли F32; актуальные инструкции обновить при реализации.

**Правило запусков:** один CPU и один GPU проход на выбранный тест/версию;
совместимый сохранённый F64 результат повторно не считать. Новый F32 CPU
reference необходим один раз на соответствующий input/configuration. Нет
обязательных медиан, многократных прогревов или повторов «для уверенности».
Повтор нужен только после изменения, ошибки или явно испорченного измерения;
причину записать. Малые unit cases и граничные значения — проверки разных
входов, а не серия одинаковых performance runs.

## 2. Инвентаризация и карта переноса

| Область | Что проверить и перевести |
|---|---|
| `model.rs`, `lib.rs`, native encoders | DTO, поля/casts/serde/NAPI, граница number -> f32, overflow после narrowing, версии контрактов |
| `geometry.rs`, `compute/numerics.rs` | Координаты, расстояния/площади, JS-compatible rounding, F64 bit masks/exponents, нормализация, shared CPU/GPU правила |
| `block_solver.rs`, `block_solver/*` | Generator, Beam/singles/pairs, frames, caches, shortlist, scores/pruning, GPU frontier и readback |
| `solver.rs`, `solver/*`, `lazy_rank.rs` | Ordinary/aligned Board search, compact representations, ranking/bounds и все stages |
| `post_place.rs`, `post_place_refine.rs`, `post_place/*` | Полная цель, MST, hints/paths, legality identities, geometric/micro modes, streaming CPU routes |
| `micro_router.rs`, `micro_router/*` | A* costs/heuristics, queue order, cached baseline/maximum improvement, grid conversion и reservations |
| `fast_route.rs`, `passive_island_solver.rs`, `ordinary_net.rs`, `signal_path.rs` | Общие CPU вычисления и вспомогательные scorers; отсутствие случайного оставшегося F64 пути |
| `compute/gpu.rs`, `workspace.rs` | Capability requirements, 4-byte buffers/alignments, layout keys, dispatch limits и buffer lifetime |
| TS PCB geometry, generators, scoring, encoders/validators | Arithmetic ownership, Math.fround/Float32Array где нужно, единое округление, входы/выходы, signature/cache keys |
| Tests, probes, captures, experiment scripts | F32 references/типизированные буферы, новые критерии сравнения, честный backend/precision в логах |

- [ ] Инвентаризировать `f64`, suffixed literals, Float64Array, размер 8 bytes,
  `to_bits/from_bits`, `partial_cmp/total_cmp`, `==/!=`, raw `>0`, epsilons,
  округление и implicit promotions. Для каждого остаточного double объяснить
  назначение; разрешённые остатки — транспорт и адаптер раздела 2.1, таймер, внешняя F64 проверка,
  а не вычислительный путь solver.
- [ ] Из captures получить диапазоны координат, длин, площадей, weights, отдельных
  terms, score и фактических improvement margins. Миллион элементов важен для
  объёма, но сам по себе не определяет погрешность или пригодность epsilon.
- [ ] Сохранить precision audit с местами сравнения, единицами, новой политикой
  и тестом. Не начинать с глобального replace epsilon или всех `==`.

## 2.1. Локализация до narrowing: отдельный контракт границы

Нормативный порядок: **исходные binary64 координаты транспорта -> общий origin
в binary64 -> вычитание origin в binary64 -> локальный F32 DTO -> CPU/GPU F32
solver -> F32 local output -> восстановление absolute output в binary64**.
`f32(x)-f32(origin)` запрещён как способ получить исходную локальную геометрию.
Например, пара `1_000_000` / `1_000_000.001` должна остаться различимой после
вычитания общего origin, а не превратиться в две одинаковые координаты.
Если различия потеряны ещё при получении исходного JS number, локализация
их тоже не восстанавливает: область поддержки относится к доступному input.

Адаптер — узкий общий код в TS encoder/decoder или DTO boundary, выполняемый
до создания native F32 model. Ему разрешены выбор/проверка origin, перенос
абсолютных координат и восстановление output; запрещены score, candidate
selection, route evaluation или повторная high-precision legality. Весь
solver, включая CPU fallback, получает только локальные F32 данные. Не
оставлять незаявленный F64 `World` под названием transport.

Начальные независимые ограничения (подтвердить captures, не выдавать за уже
проверенную область):

- `abs(input absolute coordinate), abs(origin) <= 1e9 mm`; отдельно контролировать
  точность binary64 входа, subtraction и обратного сложения с origin. Целевой
  бюджет ошибки самого boundary adapter — не более `1e-6 mm` на координату.
- `abs(local coordinate) <= 1024 mm` после выбора origin; отдельно проверить
  разности, размеры, радиусы и intermediate products, которые могут превышать
  диапазон одной координаты. Origin выбирать около центра задачи, кратным
  **1 mm**, чтобы сохранить фазу существующей сетки 0.001 mm.
- Эти границы не взаимозаменяемы: большое абсолютное смещение маленькой платы
  допустимо, маленькое смещение большой платы не доказывает достаточность F32.
  Выход за начальные границы требует обоснованного изменения представления
  (например, отдельных frames) или явного незакрытого случая, не молчаливой
  потери точности/расширения epsilon. Поддерживаемые ранее inputs не исключать
  из приёмки только для прохождения новых guards.

- [ ] Перед narrowing перенести **все** абсолютные coordinates: components,
  board bounds/outline/holes/regions, endpoints, absolute hints, pivots и
  locked poses. Не вычитать origin из уже локальных pad offsets/размеров.
  Отдельно проверить переходы block -> board и ordinary/aligned/refine frames.
- [ ] Сохранить origin/исходные locked poses как транспортные metadata;
  round-placement работает на локальной сетке с прежней фазой. Native capture
  хранит canonical local DTO и frame metadata; восстановление не должно
  зависеть от случайного текущего origin другого вызова.
- [ ] Оценить boundary roundoff, narrowing и обратный output roundoff отдельно.
  Сохранение исходных locked poses проверять на encode/solve/decode;
  не добавлять интервальные оболочки геометрии (раздел 3.2).
- [ ] Проверить одинаковую задачу без смещения и с ±1e6/±1e9 mm, различия 0.001 mm
  и меньше, half-grid, locked/unlocked соседей и полный encode/solve/decode.
  Сравнить восстановленную относительную геометрию и hard decisions; различия
  входного binary64 представления включить в бюджет. Отдельные tests — большая
  local extent при нулевом origin и границы обоих допустимых диапазонов.

## 3. Численная политика и заранее выбранные допуски

Это **начальные значения реализации**, подлежащие проверке на captures и
границах; они не объявлены экспериментально доказанными. Менять их только по
конкретному failing case с геометрическим смыслом, не подгонять под F64 score.

F32 machine epsilon = `2^-23 = 1.1920929e-7` около 1. Это не универсальный
геометрический допуск. `ULP(x)` ниже — расстояние между соседними конечными
F32 около масштаба `abs(x)`; на границе binade использовать больший соседний шаг.
Для длин масштаб берётся из исходных локальных координат/операндов, а не только
из малого результата вычитания больших чисел.

| Назначение | Начальная политика | Ограничение |
|---|---|---|
| Численная близость координат/длин | `eps_len = max(1e-5 mm, 4*ULP(M))`, M — максимальный локальный масштаб операндов | Диагностическая отправная точка; не заменяет прежние геометрические допуски и не добавляется к clearance |
| Контроль пригодности масштаба | Цель `eps_len <= 0.0005 mm`, половина существующего placement quantum | Если не выполняется, сначала локальная система координат/точные дискретные представления; не расширять clearance tolerance молча |
| Placement quantum | Сохранить `0.001 mm` там, где округление уже есть | Не округлять все footprints/pads/locked input до этого шага |
| MST tie и существующий refiner domain EPS | Сохранить `0.001` в соответствующих единицах назначения | Отделить геометрический tie, score tie и hard tolerance разными именами; не распространять число на весь проект |
| Cross product/orientation | Порог ошибки по произведениям: `8*f32::EPSILON*(abs(ax*by)+abs(ay*bx))`; геометрическая близость линии отдельно `eps_len*length(edge)` | Историческое предложение для анализа; в этой миграции прежний predicate сохраняется, глобальная замена не выполняется |
| Squared distance near boundary r | Перевести длиновой допуск в квадрат: `2*abs(r)*eps_len + eps_len^2` | Не переиспользовать линейный epsilon для squared value |
| Безразмерный t в пересечении | Начальный `eps_t=8*f32::EPSILON`, плюс оценка обусловленности знаменателя | Почти параллельные линии обрабатывать отдельно, не считать этот порог универсальной гарантией |
| Отчёт о CPU/GPU F32 score | Близость `max(1e-3,4*ULP(max(abs(a),abs(b))))` | Сигнал диагностики; не допуск для hard constraints и не основание отвергнуть хорошую геометрию |
| Принятие улучшения | Сохранить смысл `min_delta`, порогов и стабильного порядка кандидатов | Не вводить глобальный `relative_eps*total_score`, скрывающий единицы полезного улучшения на больших scores |
| Pruning/maximum-improvement bounds | Отдельная консервативная оценка ошибки выражения; в сомнительной зоне **не отсекать** | Diagnostic score tolerance не является доказанным pruning margin |

При накоплении N terms оценку порядка `gamma_n = n*u/(1-n*u)` с
`u=2^-24` можно использовать лишь там, где соблюдены предпосылки (n*u<1,
нет overflow/underflow и известна сумма абсолютных terms). Нельзя переносить
эту оценку вслепую на MST, геометрические ветвления и длинный весь pipeline.

**Жёсткая геометрия.** Имеющиеся физические clearance/hole/outline constraints
и исходные domain tolerances сохраняются. Численная неопределённость не
разрешает новую физическую коллизию. Broad phase расширяет зону проверки,
а не зону допустимого пересечения. Для пограничных cases определить единый
CPU/GPU predicate: работать в локальных координатах, использовать точные
integer predicates там, где геометрия действительно дискретная; иначе
консервативно не принимать неразрешённый кандидат. Не считать CPU F32 fallback
способом восстановить потерянную точность. Если поддерживаемый ранее input
нельзя корректно обработать, это незакрытый случай миграции, а не повод молча
обрезать координаты или включить F64.

**Score и суммирование.** Около 16 млн шаг F32 уже примерно 1; это не запрещает
F32, но дробные улучшения могут исчезать. Сначала проверить реальные margins
и конечную геометрию. При подтверждённой проблеме использовать pairwise либо
компенсированное суммирование в F32, локальные разности/масштабирование всех
связанных weights, min_delta и bounds согласованно. Не добавлять это заранее
во все kernels и не оставлять скрытый F64 sum. Равноценные расхождения поиска
сами по себе не являются проблемой.

**Равенство и сортировка.** Float можно сравнивать точно там, где нужна именно
идентичность: finite canonical stored pose, cache key, дискретный угол или
повторно прочитанное значение. `abs(a-b)<=eps` не транзитивно, поэтому его
нельзя вставлять в `Ord`/heap/sort comparator. Для сортировки finite F32 —
полный порядок и стабильный candidate ID; для domain tie — отдельное правило
выбора, не нарушающее контракт сортировки. Exact-bit caches допустимы после
нормализации signed zero, если его знак не имеет доменного смысла; approximate
cache keys не должны смешивать разные геометрии. Неожиданные NaN/Inf в
координатах и рассчитанных contributions запрещены; штатные sentinels имеют
отдельный контракт (3.3), NaN никогда не является «равным score».

## 3.1. Исполнение F32: FMA, subnormals и rotations

Согласованная пользователем 2026-10-01 единая политика — RTE для обычной F32
арифметики, FTZ: субнормальные входы и результаты обнуляются с сохранением
знака нуля; неявный FMA contraction/reassociation запрещён. RTX 3060 через
Vulkan сообщает `DenormPreserve=false`, поэтому первоначальная preserve
политика заменена явно. Изменение касается величин `0 < abs(x) < 2^-126`
(примерно `1.17549435e-38`); оно не разрешает расширять геометрические допуски.
Ошибку каждого flush ограничивает `2^-126` в единицах операции; последующее
масштабирование или деление требует учёта этой ошибки. Малость промежуточного
значения сама по себе не доказывает малость конечного эффекта.
Это относится к placement rounding, coordinate transformations, orientation
cross products, distances/squared distances, scores и pruning bounds.
Fused operation разрешается только как явно выбранная одинаковая операция
CPU/GPU с отдельной оценкой ошибки и тестом. Не считать выражения `a*b+c`
и `mul_add(a,b,c)` взаимозаменяемыми. Domain half-tie placement rounding остаётся
отдельным правилом поверх обычной RTE арифметики.

- [ ] Проверить возможность запросить через используемый CubeCL/wgpu/Vulkan
  стек выбранные execution modes: `RoundingModeRTE`, `DenormFlushToZero`,
  `SignedZeroInfNanPreserve`, а для чувствительных выражений запрет contraction
  (например, SPIR-V `NoContraction`). Наличие properties означает поддержку,
  а не то, что режим уже включён. Сохранить compiler flags и проверить emitted
  shader/IR и реальные probes, а не только прочитать capability bit.
  На текущем RTX 3060 оба denorm properties равны false. Реализация FTZ поэтому
  канонизирует F32 входы/результаты целочисленными bit operations в SPIR-V;
  `DenormFlushToZero` execution mode не запрашивается без поддержки.
  RTE/сохранение специальных значений и NoContraction запрашиваются явно.
  CPU scoped FTZ/DAZ и реальные GPU probes пройдены; full quality/performance
  приёмка этой реализации остаётся открытой.
- [ ] В capability report добавить соответствующие Float32 properties и
  ограничения independence. Одного наличия F32 недостаточно. Если режим не
  поддержан или стек не позволяет его гарантировать, GPU не допускается для
  этого контракта: полный CPU F32 fallback с причиной. Это limitation,
  не выполненная GPU performance-приёмка.
- [ ] CPU tests проверить на solver threads: FTZ/DAZ должны согласованно обнулять
  subnormal inputs/results, включая цепочку, где масштабирование могло бы
  вернуть маленький input в normal range. Не менять floating environment Node
  process; если нужен scoped guard, проверять восстановление thread state.
  Включить цепочку subnormal -> normal через последующее умножение: проверки
  только финального маленького результата недостаточно.
- [x] Пользователь явно согласовал общий FTZ CPU/GPU после проверки отсутствия
  Vulkan F32 preserve на RTX 3060. Оценка ошибок и quality evidence остаются
  обязательными; согласование режима не является завершённой приёмкой.
- [ ] Для кратных 90° использовать точные перестановки/смены знака локальных
  координат, без `sin_cos`; прибавление pivot всё равно учитывает roundoff.
  Для остальных текущих integer-degree углов использовать общую таблицу
  canonical F32 коэффициентов для нормализованных 0..359° (фиксированные bits),
  передаваемую/используемую обеими сторонами. Коэффициенты проверить один раз
  внешним высокоточным инструментом; их ошибка входит в transform bounds.
  Runtime platform `sin_cos` не считать источником одинаковых CPU/GPU bits.
  Если есть API с нецелым углом, отдельно определить поддерживаемый алгоритм,
  не округлять такой угол молча до integer table.
- [ ] Probes: contraction-sensitive multiply/add, cancellation, ±0, smallest
  normal/subnormal и переходы между ними, multiplication/division/sqrt,
  quarter turns и произвольные angles. Привязать к driver/device/build manifest.
  Не требовать от transcendental operations точности, которой API не обещает.

Vulkan перечисляет отдельные Float32 возможности для denorms, rounding и
сохранения специальных значений; их доступность и фактическое применение
проверяются отдельно. См. [float controls](https://docs.vulkan.org/refpages/latest/refpages/source/VkPhysicalDeviceFloatControlsProperties.html).
Rust не задаёт фиксированную межплатформенную точность
[`sin_cos`](https://doc.rust-lang.org/std/primitive.f32.html#method.sin_cos),
поэтому canonical coefficients — часть нашего контракта, не свойство std.

## 3.2. Геометрия: сохранение прежнего поведения

По последнему указанию пользователя новая система интервальной физической
геометрии исключена из миграции. Сохранить прежние тела, площадки, преобразования
и предикаты с F32 операндами. Не расширять boxes, не добавлять метаданные ошибки
к каждому объекту и не менять правила допуска кандидатов ради общей теории
погрешностей. Это не заявляет математической гарантии для любой пограничной
геометрии: конкретные дефекты проверяются отдельным воспроизводимым тестом.

Локализация исходных координат до narrowing и сохранение authored locked output
остаются. Проверки сравнивают реальную конечную геометрию. Численные bounds для
существующего pruning остаются отдельной задачей: они не меняют размеры объектов
и не должны отсекать потенциально лучший кандидат из-за округления F32.

## 3.3. Штатные sentinels и неожиданные NaN/Inf

Сохранить смысл существующих `+Infinity`/`-Infinity`: начальные extrema,
нет ограничения, unbounded/reject-all pruning и другие документированные
управляющие состояния. Например, `ScoreWindow::ceiling` в `block_solver.rs`
возвращает разные infinities по заполнению окна и hard rank; замена их одним
finite maximum меняет алгоритм. Геометрический input и фактически рассчитанные
score contributions должны быть конечными, кроме явно перечисленных
контрактом результатов «отсутствие геометрии/границы».

- [ ] Составить реестр sentinel producers/consumers, включая API optional bounds,
  empty geometry и `ScoreWindow`. На каждом месте либо сохранить IEEE sentinel
  с нужными float controls, либо явно заменить tagged enum/flag с тем же смыслом
  CPU/GPU. Не заменять blanket `is_finite` проверкой все управляющие значения.
- [ ] Отделить sentinel от вычисленного overflow. Проверять finite contributions
  до объединения с управляющим состоянием; не допускать `Inf-Inf`, `0*Inf` и
  передачи sentinel в distance/geometry routines. NaN не является sentinel.
- [ ] Сравнения sentinel могут быть точными и не используют epsilon. Sorting
  finite candidates отдельно от управляющих states; не терять специальные
  правила ±Infinity при переносе сравнения на F32.
- [ ] Тесты: пустое/частично заполненное shortlist, hard-rank меньше/равен/больше,
  unlimited bound, empty extrema, все кандидаты отвергнуты, настоящий overflow
  и accidental NaN. Проверить прежний контроль потока и корректный CPU replay.

## 4. Округление и граница TypeScript/native

- [ ] Сохранить правило существующего `js_round`: half ties направлены к +∞,
  включая отрицательные числа. `f32::round()` имеет другую политику и не является
  автоматической заменой. GPU F64 helpers с 52-bit significand/u64 masks
  переписать под F32, а не просто заменить сигнатуру.
- [ ] Определить одну последовательность операций для CPU/GPU: boundary localization
  до narrowing по 2.1, narrowing локального входа,
  F32 умножение на 1000, обработка half tie, integer tick и обратное F32
  представление. Контрольные сравнения округления требуют одинаковых ticks
  CPU/GPU F32, но не всех прежних F64 bits. FMA contraction в этой операции
  должен быть явно согласован; fast-math не включать одновременно с миграцией.
- [ ] Проверить half ±1 ULP для обоих знаков, signed zero, границы binades,
  края допустимого масштаба, repeated encode/decode и idempotence округления.
  Если integer ticks использованы, проверить range/overflow и то, что соседние
  разрешённые ticks не сливаются в F32 в поддерживаемом диапазоне.
- [ ] Разделить F32 канонизацию и output decimal formatting. JS arithmetic,
  производящая геометрию для solver, должна повторять нужные F32 операции;
  измерить накладные расходы Math.fround, избежать лишних TS/native crossings.
- [ ] Locked poses не перемещать из-за quantization: вычислительная копия F32,
  output исходных запрещённых к изменению poses сохраняет оригинальные значения
  транспорта. Проверить восстановленные poses на исходной геометрии;
  отдельная система enclosure не входит в объём по 3.2.
  Публичные JSON inputs и authored precision не переписывать ради fixture match.
- [ ] Все caches/signatures строить от согласованного canonical representation;
  точно проверить отрицательные координаты, rotations, opposite-side pads,
  слой по умолчанию и repeated conversion.

## 5. Реализация CPU F32 и GPU F32

- [ ] Сначала единый numeric contract и CPU F32 по всей цепочке, затем согласованные
  GPU consumers. Промежуточную mixed ветку не выпускать; финальная сборка содержит
  только F32 solver. Не поддерживать два общих generic precision backend.
- [ ] Сохранить полный Beam -> singles -> pairs/checkpoint цикл, ordinary/aligned
  board calls, portfolio decisions и geometric/route-aware refinement. F32 не
  оправдывает урезание стадий или произвольный geometric top-K перед Micro-A*.
- [ ] Перевести GPU templates, frames, terms, scores, upload/readback и kernels;
  пересчитать buffer strides, alignments, scratch sizes/capacity и layout keys.
  Массовый GPU scorer не дополнить скрытым массовым CPU rescoring.
- [ ] Пересмотреть F64/u64 capability requirements. F64 не требуется новой
  реализации; u64 оставить только если реально нужен целочисленным операциям.
  Отсутствие GPU F64 не должно отключать F32 backend; float controls по 3.1
  остаются независимыми требованиями. Не заявлять проверку
  устройства без F64 только на основании запуска на RTX с F64.
- [ ] Сохранить resident lifetime, защищённый scratch/readback, OS lease и общий
  runtime между block/board/post-place. Обновить numerical guards под F32
  intermediate products, суммы и накопление, не только входной диапазон.
- [ ] GPU failure: полный исходный block/board/refine replay на CPU F32; для
  refiner исходный timeout заново. Отбрасывать частичные GPU poses/checkpoints,
  освобождать lease и scratch, не повторять GPU в этой попытке.
- [ ] После замеров заново оценить auto thresholds/chunk sizes. Старые F64 пороги
  производительности не считать оптимальными F32. Не увеличивать CPU workers
  или менять бюджеты только ради красивого сравнения.

## 6. Проверки чисел, алгоритма и восстановления

- [ ] Unit matrix: localization до narrowing и translation cases (2.1),
  actual float controls/rotations (3.1), полная error chain (3.2), sentinels (3.3),
  ±0, finite extrema поддерживаемого диапазона, narrowing
  overflow, NaN/Inf, tiny values/underflow, cancellation, суммы больших и малых
  terms, squared distances, почти parallel/collinear/zero-length geometry.
- [ ] Hard boundaries: касание и зазор с обеих сторон порога, старые C20/C30 и
  C30/C21 F32 cases, holes/outline/polygons/regions, layers/through-hole pads,
  locked inventory, rigid groups, все разрешённые rotations. Не заменять
  совпадение violation identities сравнением только количества нарушений.
- [ ] CPU/GPU F32: все score terms, MST ties и empty/singleton/large nets,
  zero/negative weights по действующему контракту, paths, pad dedup, foreign-pad
  movement, cache invalidation. Проверять actual GPU kernels. Допустимое
  расхождение итогового поиска не скрывает пропущенный term или неправильный
  знак/масштаб contribution.
- [ ] Pruned/unpruned на малых диагностических inputs: безопасное отсечение не
  теряет доказанно лучший по тем же F32 правилам вариант вне tie zone. В tie
  zone сравнить допустимость/качество, а не требовать прежний F64 candidate ID.
- [ ] Stable sort/heap ordering, no unexpected NaN/Inf, штатные sentinels,
  repeated-state handling, termination,
  сохранённый route-only winner. CPU/GPU могут выбрать разные равноценные
  пути, но не терять обязательную стадию или маршрутное обязательство.
- [ ] Chunks 1/неполный/default, workers 1/2/4 и штатный budget, последовательные
  mixed block/board/refine calls и конкурирующие процессы. Проверить отсутствие
  oversubscription/новых runtime и чтения перезаписанного scratch.
- [ ] Disabled/no-device/busy/unsupported, capability F32-without-F64 guard,
  failure до/в batch, после принятого move и перед diagnostics. Повтор CPU F32
  соответствует исходному input; свежий timeout и release lease проверены.
- [ ] Zero/small/production timeouts, no-improvement, iteration-limit, interrupted
  batch; результат целостный, cooperative overshoot измерен. Не требовать
  identical moves у разных timed runs.

## 7. Приёмка по геометрии и полным проходам

Сохранить две независимые оси: **F64 -> F32 качество** и **CPU F32 -> GPU F32
корректность/скорость**. Для другого дерева поиска нельзя объявлять «тот же
объём работы» только по одинаковому названию fixture.

| Набор | Обязательный объём |
|---|---|
| FPGA + ещё два сохранённых блока разного размера | Полная сборка, включая singles и применимые pairs; один маленький/USB и один средний случай, зафиксировать реальные имена captures |
| Telemetry Board Packager | Полный ordinary + aligned native pair, ранжирование и итоговый выбор |
| Telemetry/ESPower final refiner | Полный законченный native поиск со штатными настройками, не только score kernel |
| FPGA/USB local refiner | Полный geometric вызов, включая initial/final/diagnostics |
| ESPower и esp32c3 | Два полных TS pipeline: native stages, portfolio, post-place, финальная геометрия и SVG |
| Telemetry или PortableScope | Один заранее выбранный тяжёлый полный pipeline; сохранить достигнутые стадии/stop reason, незавершённый прогон не считать приёмкой |
| Один timeout-quality input | Одинаковый штатный budget CPU F32/GPU F32; больше candidates не подменяет качество |

F64 saved references используются где совместимы; при отсутствии тяжёлого
полного F64 результата не выдумывать speedup. Сначала один необходимый baseline
или явно ограничить сравнительный вывод, при этом новый F32 full pipeline всё
равно проверить. Все fixtures/solver settings фиксируются до измерений.

Для каждого результата сохранить poses/слои/ориентации, inventory, hard violation
identities и `placementOk`, clearance minima, outline/hole/region checks,
route feasibility/unresolved/budget-exhausted по приоритетам, envelope area,
HPWL/доступные wire/path metrics, score breakdown, accepted moves/passes,
completed stages, stop reason, CPU/GPU fallback и wall time. HPWL не называть
длиной реально проложенных трасс. Сравнить previews; визуальная оценка дополняет
численные проверки, не заменяет их.

При изменившейся расстановке проверить **конечные output poses на исходной
геометрии** отдельно от rounded internal boxes. Внешний старый F64 validator/
scorer допускается как измерительный инструмент в другом процессе; новый
production код его не содержит. Сравнивать F64 и F32 решения одним внешним
измерителем и текущим F32 validator, не только двумя собственными scores.
Пограничные расхождения старого validator сверять с физической геометрией и
исходным tolerance; raw-float микропересечение не объявлять автоматически
реальным дефектом, но и не скрывать увеличением допуска.

**Решение о качестве:**

- Hard constraints, inventory, разрешения fixed/rigid/layer сохраняются.
  Известные исходные violations/debt не объявлять исправленными; новые не
  маскировать равным count или изменением fixtures.
- Близкий score при другой хорошей геометрии — допустимо. Небольшая потеря
  одного soft показателя при улучшении другого отражается в отчёте и не
  запускает автоматически «исправление F32». Прежний FPGA пример
  +0.0315% score при улучшившемся HPWL — полезный пример, не универсальная граница.
- Для сортировки отчёта выделять изменения soft metrics больше 1% и любые
  изменения route feasibility для предметного разбора. 1% — порог внимания,
  **не разрешённое ухудшение каждого ограничения** и не автоматический отказ.
- Исправлять доказанную физическую/алгоритмическую ошибку или существенное
  необъяснённое ухудшение. Не расходовать новые полные прогоны ради воспроизведения
  F64 winner, десятых знаков score либо byte-identical SVG.

## 8. Скорость, память, сборка и закрытие

- [ ] Измерить full wall и этапы отдельно: encoding/generation, legality,
  score/batches, transfers/readback, routes, diagnostics. Не складывать
  overlapping worker time в «полное время» и не выдавать scorer speedup за
  full solver speedup. Cold и ready runtime различать, startup не прятать.
- [ ] Записать число реально проверенных кандидатов, стадий и качество. При
  различающейся траектории полный wall — практическое сравнение решения той же
  задачи; kernel comparison на одинаковых сохранённых batches — отдельный тест.
- [ ] Память: peak process working set/private bytes, tracked resident/scratch
  CPU/GPU bytes, upload/readback bytes, доступная VRAM telemetry с указанием её
  ограничений. `workspaceBytes` не равно полной VRAM. Для float buffers ожидается
  8 -> 4 bytes; whole process/peak workspace не обязаны уменьшаться ровно вдвое.
- [ ] Не запускать обе тяжёлые версии одновременно. Сравнивать release builds,
  одно устройство, одинаковую нагрузку/worker budgets/профилирование. Не вводить
  серию microbenchmarks на миллион значений вместо приёмки реальных задач.
- [ ] `npm run native:build`; Rust tests с GPU и `--no-default-features`;
  focused PCB score/geometry/block/board/refiner/route/capture tests;
  `npm run typecheck`, `npm run build`, package/native loading. Фактический список
  файлов тестов сохранить; broad suites не дублируют уже законченные perf runs.
- [ ] Проверить addon size/imports, CPU-only load, настоящие GPU kernels, один
  `.node`, отсутствие новых DLL/JIT. Audit source и GPU representation показывает
  F32 вычисления, корректные strides и отсутствие legacy F64 solver.
- [ ] Обновить current API/numerics/AGENTS и changelog, backend/precision logs,
  auto admission evidence. Исторические F64 results остаются историческими.
- [ ] Создать `F32_MIGRATION_RESULTS.md` с baseline/F32 revisions и hashes,
  точными командами, input IDs, single-pass matrix, quality verdict по каждому
  case, memory/stage/full times, известными ограничениями и final artifact.
  Generated файлы — `debugging/f32-migration-<date>/`, отдельные неизменяемые
  каталоги для версии/конфигурации. Закрывать checklist только с evidence.

**Готово**, когда новая ветка содержит только согласованный F32 вычислительный
путь CPU/GPU, сохранены физические и алгоритмические обязательства, проверены
полные перечисленные циклы/восстановление и опубликованы реальные speed/memory
результаты. Сходство score в последних знаках и одинаковые F64 poses не нужны.
Недостигнутые 2× не скрывать; обоснованный выигрыш оценивать по реальному времени,
памяти и качеству. Измеренный заметный регресс отдельного штатного случая
разобрать до объявления миграции успешной, не скрывать средним по набору.

## Основания

- [Исторический F32 FPGA эксперимент](../block-precision-f32-2026-09-30/README.md):
  другое дерево поиска при близком качестве и разрыв штрафа на микрозазоре.
- [Refiner results](POST_PLACE_GPU_RESULTS.md), [Board results](BOARD_PACKAGER_GPU_RESULTS.md),
  [block results](RESULTS.md), [общий GPU API](GPU_INFRASTRUCTURE_API.md).
- [Rust f32](https://doc.rust-lang.org/std/primitive.f32.html): binary32,
  machine epsilon, rounding, finite/NaN и total ordering.
- [NVIDIA Floating Point](https://docs.nvidia.com/cuda/floating-point/index.html):
  порядок операций, FMA и накопление погрешности. Числа domain tolerances выше —
  проектные решения этого roadmap, а не рекомендации производителя GPU.
