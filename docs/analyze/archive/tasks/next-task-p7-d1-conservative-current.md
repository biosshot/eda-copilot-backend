# P7-D1C: консервативный normal-current reference

2026-10-31. Один исполнитель. Родитель повторил D1R replay и сохранил его checkpoint `c12701f`. Это ограниченная работа над joint reference/current formulation. Не hybrid, не новый public solver и не этап E/F/G.

## Обязательный контекст

Прочитать `AGENTS.md`, `CHANGELOG.md`, затем:

- `docs/experimental/analyze/chat-handoff-2026-10-31.md`;
- `docs/experimental/analyze/p7-plan.md`;
- `docs/experimental/analyze/next-task-p7-d1-coupled-assembly.md`;
- `docs/experimental/analyze/p7-d1-coupled-assembly-results.md`;
- `docs/experimental/analyze/next-task-p7-d1-reference-recovery.md`;
- `docs/experimental/analyze/p7-d1-reference-recovery-results.md`.

Исходный addon SHA256: `f15e8aee3c87da0847902c571199a9bc2eaa5f116bf3bb957f686fb9d727ee73`. Не reset/clean, не удалять пользовательские данные и `debugging/`. Не создавать субагентов, не коммитить и не пушить. Все новые локальные результаты — только в `debugging/p7-d1-conservative-current/`.

## Уже доказано, не перепроверять тяжёлыми прогонами

На fixed slot при токе электродного баланса около `1 A` исходный full-space harmonic DC current lift даёт через середину обхода только `0.4162806829 A`; continuous control даёт около `0.999565 A`. Reduced reconstruction, source mapping, units, section area и complex phase этот дефект не создают. Причина локализована в mixed P1-potential / independent constant-vector-current представлении: weak nodal KCL не обеспечивает continuity нормального тока на гранях tetrahedra.

Magnetic patch1→2 остаётся независимым отказом `10.2046289% > 2%`. Прежние hybrid discrepancies не являются доказанной ошибкой hybrid, пока joint reference недопустим.

## Цель и допустимый результат

Получить **одну математически обоснованную** консервативную normal-current formulation или equilibrated recovery на существующей conforming copper mesh. Она должна позволять использовать complex section flux до/внутри/после щели как проверяемый физический observable, без эмпирического масштабирования.

Положительный результат этого задания — только новый joint reference candidate с доказанным current conservation. Он не закрывает D1/P7/P8/P11 и не доказывает magnetic/domain convergence автоматически.

Если корректная реализация требует замены всего field solver, неограниченного глобального factorization или серии методов — остановиться с NO-GO и точным архитектурным blocker. Не перебирать методы ради численного pass.

## Обязательная последовательность

### 1. Зафиксировать постановку до production-правки

Сначала изучить существующие `volume_current.rs`, `volume_solve.rs`, `volume_reduction.rs`, `volume_magnetic.rs`, `board_prepare.rs`, mesh/face ownership и tests. Искать существующие helpers перед добавлением новых.

В новом отчёте заранее записать:

- выбранные DOFs и пространство тока;
- sign/orientation и единицы face flux, cell density, electrode injection;
- какие weak KCL/KVL, nodal reactions, port balance, losses и energy сохраняются;
- как обеспечивается равенство нормального потока на внутренних гранях и физические условия на copper boundary/electrodes;
- почему интеграл выбранного поля через произвольное mesh-aligned сечение обязан совпадать с electrode balance;
- отличие postprocessing recovery от изменения исходной PDE/magnetic formulation.

Допустим один выбранный путь, например обоснованная H(div)/RT-подобная equilibrated recovery, **если** она действительно совместима с текущими данными и guards. Нельзя просто назвать существующий constant vector «H(div)» или нормировать его на нужный ампераж.

### 2. Сначала маленькие независимые tests

До slot/continuous run добавить focused tests для выбранного метода:

- constant/affine manufactured current с известным flux;
- два и более tetrahedra с общей гранью: equal/opposite normal flux и сохранение заданных nodal reactions;
- нетоконесущие внешние copper faces без выдуманного утечечного потока;
- electrode flux и суммарный баланс, включая complex real/imaginary состояние;
- determinism, nonfinite/range refusal и отсутствие изменения физической геометрии.

Если recovery меняет energy/loss/magnetic path, нужны отдельные before/after controls, показывающие, что именно изменилось. Нельзя скрыто подменить старую энергию новой и объявить прежний reference исправленным.

### 3. Минимальная реализация только в существующем пути

Разрешены точечные изменения в `native/analyze/src/extraction/local/` и соответствующие diagnostic fixtures/tests. Не менять hybrid/interface moments, section models, P6, DSL, guards, tolerances, geometry fixtures или solver resource policy.

Не добавлять trace moments, empirical correction, global plane L/C, новый selector, F32/GPU, out-of-core, external solver или OS paging. Не менять исходный mixed solve, если выбранный консервативный postprocessor уже даёт корректный observable; при этом явно ограничить область доказанного результата.

Сохранять совместимость старых saved packets там, где это возможно. Если новый full-space evidence невозможно получить из старых packets, не выдавать reduced replay за full-space проверку.

### 4. Полный loaded DC/1 kHz evidence

После unit/focused tests допустим один bounded slot run и один continuous run на неизменных fixtures:

- `tests/fixtures/analysis-p7-joint-section/slot.json`;
- `tests/fixtures/analysis-p7-joint-section/continuous.json`.

Сечения заранее фиксированы: `x=0.500123`, `1.000123`, `1.500123 mm`, физическая нормаль `+x`. Сохранить exact full-space loaded DC и 1 kHz potential/current/conservative-face states либо достаточный набор для независимого replay. Сравнить full и reduced при одинаковом four-port RMS Norton drive `d=[1,0,-1,0] A` и `50 ohm` на каждом порту.

Для каждого сечения записать complex BOTTOM/TOP/all-conductor flux и electrode balance. Математические continuity/electrode constraints должны выполняться с нормированной ошибкой не более `1e-8`; full-versus-reduced current/observable check использует существующий численный gate `2%` с абсолютным current floor `1e-6 A`. Это не разрешение объявить физическую точность 2%.

Один heavy run одновременно. Каждая попытка <=300 s и <=2 GiB private/RSS с host headroom; один cold slot и один cold continuous максимум, без автоматической серии refinement. Не собирать native addon параллельно численным прогонам. Не запускать STM32, direct DSL, hybrid, mesh profile3 или повторные unchanged failures.

Если изменился magnetic/numeric path, не начинать новую patch/domain кампанию автоматически: записать необходимые rerun gates и остановиться после разрешённых current controls. Если magnetic path не изменился, прежний `10.2046289%` остаётся отдельным blocker.

## Проверки и provenance

При Rust-изменении выполнить `npm run native:build`, focused Rust tests, соответствующие TS tests, `npm run typecheck` и `git diff --check`. Записать новый addon SHA256, source/core identity, hashes входов и packets, cold/warm/reused статус, wall time и доступные resource measurements. При отсутствии toolchain или нехватке evidence честно указать unverified; не выдавать старый binary/cache за новый результат.

Графики построить по новым audits и открыть. Старый `reference-current-audit.png` можно использовать только как marked baseline, не как результат нового кода.

## Отчёт и остановка

Создать `docs/experimental/analyze/p7-d1-conservative-current-results.md` с постановкой, уравнениями, выбранным методом, причиной совместимости, diff, tests/hashes/resources, таблицами complex flux и PASS/NO-GO по каждому gate. Обновить `CHANGELOG.md` при реальном изменении behaviour/verification workflow.

Не менять общий статус P7. Не начинать hybrid comparison после local PASS. Остановиться для родительского ревью с одним из двух итогов:

1. **current-conservation PASS, reference still conditional** — если выполнены только current gates, но magnetic/domain ещё открыты;
2. **NO-GO** — если консервативная постановка не реализована или не проходит проверки, с минимальным воспроизводимым blocker и без перебора альтернатив.
