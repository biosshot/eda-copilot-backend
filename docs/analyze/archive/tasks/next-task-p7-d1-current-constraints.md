# P7-D1: независимые ограничения conservative current

2026-10-31. Один исполнитель. Продолжение только текущего blocker D1C, не новая solver campaign.

## Контекст пользователя и цель

Цель проекта — быстрый экстрактор параметров выбранных каналов/групп/PDN из произвольной PCB для последующих анализов с минимальными временем и памятью. Однородные участки должны использовать быстрые section models; 3D local только для обоснованных неоднородностей. Текущая работа исправляет prerequisite для сложных возвратных путей, а не меняет цель на DC-анализ.

Пользователь разрешил выдавать достаточно близкие приближённые результаты с явным warning и измеренной погрешностью; сверхточность не всегда нужна. Это не разрешает скрывать unknown error, failed solve или менять guards в данном задании. Algebraic current conservation остаётся обязательным для выбранного reference observable.

## Прочитать

`AGENTS.md`, `CHANGELOG.md`, `p7-plan.md`, `p7-d1-reference-recovery-results.md`, `next-task-p7-d1-conservative-current.md`, `p7-d1-conservative-current-results.md`.

Checkpoint D1C: `a483f10`. Не reset/clean, не удалять `debugging/`, не создавать субагентов, не коммитить и не пушить. Новые локальные artifacts — `debugging/p7-d1-current-constraints/`.

## Точный исходный дефект

D1C добавил diagnostic-only RT0 minimum-correction recovery. Seventeen focused tests, native build и typecheck проходят. Первый cold slot остановился за `11.625 s` с:

`LOCAL_CONSERVATIVE_SINGULAR: independent equilibration constraints are not positive definite; no regularization`.

В `ConservativeSpace::new` DC projector создаётся до AC, поэтому это отказ именно DC constraint factor. Full loaded states не созданы; continuous не запускался. Старые mixed PDE, RLC/loss/magnetic/energy paths не изменены. Magnetic patch `10.2046289% > 2%` остаётся отдельным blocker.

Реализация: `native/analyze/src/extraction/local/conservative_current.rs`. Constraints содержат все quotient-node reactions и все cell-divergence rows кроме одной cell row на connected component. Комментарий предполагает ровно одну dependency на component; на реальном slot это не подтверждено.

## Цель

Установить точную причину rank deficiency и, если она является дефектом выбора/реализации ограничений, минимально исправить **тот же RT0 recovery**. Не выбирать другой solver/recovery family и не применять regularization/pseudoinverse для сокрытия дефекта.

Положительный итог — current-conservation PASS на slot и continuous, но reference остаётся conditional до отдельных magnetic/domain gates. Это не D1/P7/P8/P11 acceptance и не hybrid comparison.

## Обязательная последовательность

### 1. Rank/dependency audit без нового FEM

Использовать сохранённый slot packet `debugging/p7-closure/1793437157149686200/joint-fine.json` и существующие mesh/current/electrode данные. Не запускать magnetic/electric solve для диагностики.

Независимо построить ту же DC constraint matrix `C` и вычислить dependencies. Коэффициенты структурные (`0`, знаки, `1/3`), поэтому предпочтителен exact/rational/modular rank или другой проверяемый метод с независимой проверкой; одного numeric Cholesky failure недостаточно.

Записать:

- числа faces/free faces, raw/gauged rows, rank и deficiency;
- каждую найденную зависимость в терминах nodes/cells/components/electrodes;
- почему прежнее удаление одной cell row на component недостаточно или неверно;
- выполняются ли опущенные строки автоматически из сохранённых независимых ограничений;
- относится ли дефект только к slot или также к manufactured tests.

Нельзя произвольно выбрасывать строки до исчезновения ошибки. Каждая удалённая зависимая строка должна иметь математическое доказательство и пост-проверку на полном исходном residual.

### 2. Минимальная правка только после доказанной причины

Если установлена конкретная структурная ошибка, исправить выбор независимого constraint basis или assembly в существующем `conservative_current.rs`. Сохранить:

- один общий signed flux DOF на внутренней грани;
- zero cell divergence;
- исходные nodal reactions и electrode totals;
- DC insulation на неэлектродных copper boundaries;
- полный audit всех omitted rows;
- deterministic bounded sparse factor, CPU/F64 и allocation guards;
- старые mixed solver, RLC/loss/magnetic/energy paths без изменений.

Не добавлять regularization, empirical scaling, новый public selector, mesh/geometry changes, F32/GPU/out-of-core/external solver. Не менять hybrid/interface/P6/DSL.

Если причина не установлена или требуется другая formulation — NO-GO с exact blocker, без перебора альтернатив.

### 3. Focused tests

Сохранить 17 существующих tests и добавить минимальный regression, воспроизводящий найденную dependency. Проверить constant/affine modes, shared faces, electrode totals, insulation, complex AC, omitted-row residual, determinism и nonfinite refusal. При Rust-изменении выполнить `npm run native:build`, focused Rust tests, `npm run typecheck`, `git diff --check`; новый addon SHA256.

### 4. Один повторный slot, затем один continuous

Только после прохождения rank audit, build и focused tests разрешён один новый cold slot на изменённом коде. Это новая попытка по новой причине, не повтор unchanged failure. Максимум `300 s`, `2 GiB` private/RSS с host headroom, один heavy run одновременно; native build не выполнять параллельно численному run.

Если slot получает loaded full-space DC/1 kHz states и проходит algebraic current gates, выполнить один continuous control. Если slot отказал — continuous не запускать.

Фиксированные sections `x=0.500123/1.000123/1.500123 mm`, normal `+x`; drive `d=[1,0,-1,0] A`, loads `50 ohm`. Сохранить exact full/reduced potential/current/face states и independently replay complex BOTTOM/TOP/all-conductor flux. Algebraic omitted-row/continuity/electrode error <=`1e-8`; full-versus-reduced observable gate `2%`, absolute floor `1e-6 A`. Не выдавать старый packet за новый результат.

Не запускать magnetic/domain refinement, hybrid, STM32, direct DSL, profile3 или серию уточнений. Прежний magnetic blocker остаётся открытым независимо от current PASS.

## Отчёт

Создать `docs/experimental/analyze/p7-d1-current-constraints-results.md`: exact rank evidence, уравнения dependencies, diff, tests/build/hashes/resources, cold/reused provenance, complex flux tables и PASS/NO-GO по каждому gate. Обновить `CHANGELOG.md` при реальном изменении verification workflow. Построить и открыть график только если появились новые numerical audits.

Остановиться для родительского ревью. Не коммитить, не объявлять P7 закрытым и не начинать следующий этап автоматически.
