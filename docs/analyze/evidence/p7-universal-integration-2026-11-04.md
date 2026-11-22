# P7 universal integration — 2026-11-04

Статус: реализованы части W0–W4/W6; не приёмка всего P7. CPU/F64, без fast/P8/P11. Полные native-сборки выполнены штатным `npm run native:build`: исходный rebuild 6m44s, combined cross-polarity rebuild 5m55s. Первый вызвал пересборку общего packer; это не время экстрактора. `npm run build` прошёл перед D1.

## Реализация и проверки

- Общие терминалы/порты: `src/analyze/channel-terminals.ts`, `board-channel-plan.ts`. Физические ID, ориентация и несколько сигналов/возвратов сохранены. Пяти-портовая группа и трёхпортовая ветвь прошли geometry checks. Cross-block external pairs и неподдержанные физические модели не заменены искусственным соединением.
- Многопроводниковый section: `segmented-channel.ts` maps всех n+1 физических проводников при n независимых координатах относительно математического gauge. Другие возвраты сохраняют собственные потенциалы/токи. Порядок native коэффициентов проверяется; RLC и magnetic используют один incidence.
- P6: additive `positive-negative`/`negative-positive` contacts в `electrical/coupled/physical.rs` и `composition.rs`. Existing both/positive/negative semantics сохранены. Synthetic gauge/order/orientation S совпадает <1e-10, возвраты не welded.
- Реальный vacuum section, 3 проводника/3 физических порта: четыре focused W2 tests PASS, 4.93s всего, сам geometry solve 4.20s. Gauge A: level3/65151 nodes/final delta0.31184%; B: level2/16592 nodes/0.46623%. Full physical C sensitivity0.195421336%; max complex-S absolute1.561693325e-7, normalized(.01 floor)4.685079955e-7. Это numerical sensitivity PASS 2%, не независимая physical-accuracy приёмка. Первый 40k-node control отказал по cap; один bounded вариант100k прошёл. Первоначальное1e-8 absolute-S сравнение было слишком строгим для разных выбранных сеток, заменено существующим sensitivity criterion; алгебраический тест оставлен строгим.
- Split-return geometry: убраны только reference-free uniform intervals; разрыв остаётся в joint event между сечениями, две same-net return islands и электроды различаются. Дальний distributed lateral STM32 refusal сохранён. EM/capacitive board accuracy этим не доказана.
- DSL: исходный `.si().channels([...])` уже предусмотрен для DDR/групп (dsl-working-contract и scenario-decisions). `localChannelGroupRequest` готовит один совместный запрос и handle mapping; layered differential подключён к joint 4-port extraction. SI waveform/timing не реализованы. Direct differential output — физические leg-to-return S50, не mixed-mode characteristic Z.
- Geometry/terminal/DSL suite: 29/29 PASS; включает существующий source-subdivision, rotated, via, meander, lateral-refusal controls. Typecheck PASS.
- Частоты: `p7-frequency-planning.ts` сохраняет requested intervals, адаптивные midpoint holdouts, unsupported/budget/deadline/cancelled gaps. 6 focused tests PASS. Local executor использует общий parser с прежней сеткой; adaptive physical evaluator ещё не подключён к production и HF/skin operator не реализован.
- Capacitive return: `cargo test --manifest-path native/analyze/Cargo.toml --test capacitive_return -- --test-threads=1` PASS 1/1, execution0.01s, compile22.82s. Аналитические complex impedance/gap current и charge balance, DC broken/continuous. Manufactured EQS slab с insulated sides, grounded planes и нулевым magnetic action; не реальная PCB или HF-валидация.
- Manifest: все42 записи имеют missing prerequisites/observables/proposed reference; existing paths привязаны где найдены. `ready=0 pending=42` относится к подготовке нового acceptance manifest, не отменяет исторические частные приёмки. Checker и +1.5pp review tests PASS.

## D1

Один cold attempt `debugging/p7-d1-cross-coupling/slot-universal-20261104-1`, phase `dsl`, свежие addon/dist, отдельный cache, 300s/2GiB supervisor. Cold: `resource-failure`, external deadline300.046s, peak RSS127066112B, private134418432B. DSL result не получен. Основные local magnetic preparations68.41s/8.97s/64.91s; последующие совпадающие geometry kernels переиспользованы.

Один диагностический cache replay с20s внутренним/30s внешним лимитом остановлен `CHANNEL_MAGNETIC_DEADLINE`, без результата. После явного выбора пользователя выполнен ровно один resume с600s cap, тот же physical input/native и сохранённый cache: `slot-universal-20261104-resume600`. Supervisor complete87.766s, RSS145489920B/private148869120B; **DSL failed CHANNEL_DOMAIN_CONVERGENCE**. Это не cold success и не PASS D1.

Числа из полной диагностики `reports[0].diagnostics[0].message` (верхний error.message усечён): domain1 spatial max7.387335392e-5; domain1→2 delay13.739950486%; domain2→4 delay16.795619657%. Magnetic и copper-loss changes порядка1e-12/1e-11; wave changes1.5444e-5/2.2691e-5. Оба delay существенно выше2%+1.5п.п., остаются FAIL. Electric-margin patch не устранил прежний drift; последующая локализация и исправление near-air mesh описаны в [отдельном отчёте](p7-d1-slot-electric-diagnosis-2026-11-04.md). Результат последующей холодной проверки приведён ниже. Полный parsed report сохранён как `domain-verification.json`.

## Незакрытые обязательства

D1/domain и независимая композиция; полный STM32 distributed plane; удалённые возвраты; blind/buried/via-fence и board-level coverage всей матрицы; component bridges; skin transition и local HF surface-current operator; production adaptive physical solves/common band; пять реальных DSL cold/warm/restart и точность/recovery. Позитивные геометрические тесты и manufactured controls не закрывают эти пункты.
## Холодная проверка D1 после коррекции электрической сетки

Единственный согласованный холодный запуск `slot-graded-electric-cold600-20261104` завершился под supervisor за 569.391 s. DSL завершился с условным результатом в полосе 1–10 кГц. Supervisor измерял только launcher через root handle в `run_cold.py`: RSS 126 140 416 Б и private 135 016 448 Б. Вложенный lifecycle DSL зафиксировал пиковый RSS worker 424 189 952 Б (около 404.54 МиБ). Память дерева процессов и соблюдение лимита 2 ГиБ для дерева остаются непроверенными.

| Сравнение delay | Изменение | Гейт |
| --- | ---: | --- |
| Пространственное уточнение domain 1 | 0.07908538% | PASS |
| Domain 1 → 2 | 5.467882% | FAIL, выше 3.5% |
| Domain 2 → 4 | 1.291154% | PASS |
| Сдвиг границы domain 1 | 1.09686269% | PASS |

Все пять возвращённых ключей точно совпали с domain 1 fine, хотя сравнение domain 2 → 4 прошло. D1 остаётся открытым. Исправлен выбор результата в board-channel-executor: выбирается больший домен прошедшей пары, пространственное уточнение и сдвиг границы проверяются на нём же; возвращаемые модель, audit и blocks относятся к одной области. Пять целевых тестов с подставными откликами, npm run typecheck и npm run build прошли (JS 130 ms, DTS 8.086 s). Физический D1 после этой правки не повторялся; прохождение логических тестов не является его приёмкой. Разрешение на единственный холодный прогон использовано. Дальнейшие полные прогоны не запланированы.
