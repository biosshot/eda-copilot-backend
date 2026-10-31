# P7-D1: conservative recovery итогового loaded state

2026-10-31. Один исполнитель. Продолжение того же RT0 recovery после родительского ревью D1-constraints. Не новая formulation и не solver campaign.

## Контекст

Прочитать `AGENTS.md`, `CHANGELOG.md`, `p7-d1-reference-recovery-results.md`, `p7-d1-conservative-current-results.md`, `p7-d1-current-constraints-results.md` и актуальный `p7-real-channel-closure-plan.md`.

Checkpoint: `8ecb3d8`. Exact rank repair принят родителем: DC repaired matrix имеет 189 rows/rank 189, AC gauged 265/265; 18 focused tests и typecheck повторены. Не reset/clean, не удалять `debugging/`, не создавать субагентов, не коммитить и не пушить.

Пользователь подтвердил цель: быстрый экстрактор параметров произвольных PCB-каналов для последующих анализов. Достаточно близкие approximate результаты допустимы с warning и измеренной погрешностью. Данное задание не меняет accuracy policy/guards.

## Родительская коррекция предыдущего вывода

D1-constraints правильно исправил rank, но его отказ `LOCAL_CONSERVATIVE_RESIDUAL` при `project_basis(..., basis.current, true)` доказывает только невозможность рассматривать каждый auxiliary magnetic/training current mode как самостоятельное insulated DC physical field.

Это **не** доказывает несовместимость RT0 с физическим loaded solution. Auxiliary basis columns могут не иметь самостоятельного физического смысла; обязательна консервативность их фактической линейной комбинации при реальном excitation.

Нельзя повторять прежний all-mode DC projection или объявлять по нему невозможность recovery.

## Цель

Получить и независимо проверить conservative RT0 current для фактических full/reduced loaded DC и 1 kHz states на fixed slot и continuous, сохраняя прежние mixed PDE, RLC/loss/magnetic/energy paths.

Положительный итог — current-conservation PASS и conditional joint reference candidate. Magnetic patch `10.2046289% > 2%` остаётся отдельным blocker. Не D1/P7/P8/P11 acceptance и не hybrid run.

## Разрешённая реализация

### 1. Убрать неправильное требование

В `board_prepare.rs` убрать обязательную DC projection всех `basis.current` modes. Не требовать insulated DC recovery для auxiliary modes по отдельности.

RT0 recovery применять к итоговым full/reduced current vectors после loaded solve/superposition. Сохранять exact raw full/reduced current/potential и recovered face arrays для независимого replay.

AC per-mode projection можно сохранить только если она нужна и проходит без изменения физики; предпочтительный простой diagnostic путь — одинаковый state-based recovery для full/reduced DC и AC. Не делать два разных метода ради pass.

### 2. Loaded DC и AC

Использовать существующие harmonic electrode states и/или full solver, не создавая новый field solver. Зафиксировать в отчёте уравнения loaded superposition, units, signs, electrode totals и RMS convention.

Fixed four-port RMS Norton drive `d=[1,0,-1,0] A`, `50 ohm` на каждом порту. DC и 1 kHz должны соответствовать одному физическому подключению. Не смешивать internal four-port с public from/to two-port.

Full/reduced comparison выполнять при одинаковом loaded drive. Если reduced DC state сам нарушает compatibility, показать это как результат gate, а не удалять физические ограничения.

### 3. Reduced replay

Если существующий optional `board_response` conservative packet зависит от удалённого all-mode DC basis, изменить его на state-based recovery либо ограничить/удалить только этот непринятый diagnostic output с явным regression. Не оставлять malformed contract и не менять старые current/loss/port outputs.

Сохранить compatibility старых packets. Не добавлять public selector, trace moments, hybrid changes или empirical scale.

## Сначала tests

Добавить regression, где отдельные auxiliary modes намеренно не являются insulated DC fields, но их фактическая loaded комбинация совместима и успешно восстанавливается. Проверить:

- full/reduced loaded state, а не all-mode projection;
- shared-face normal continuity, zero cell divergence, electrode totals;
- DC insulation и complex AC states;
- omitted-row residual, determinism, nonfinite refusal;
- старые port/loss/RLC/magnetic paths неизменны.

При Rust-изменении: `npm run native:build`, focused Rust tests, `npm run typecheck`, `git diff --check`, новый addon SHA256. Не собирать native параллельно numerical run.

## Bounded numerical controls

Только после tests: один новый cold slot; при current PASS — один continuous. Максимум `300 s`, `2 GiB` private/RSS с host headroom, один heavy run одновременно. Новые artifacts только `debugging/p7-d1-loaded-current/`; не использовать старые failed attempts как новые результаты.

Fixed sections `x=0.500123/1.000123/1.500123 mm`, normal `+x`. Для full/reduced DC и 1 kHz записать complex BOTTOM/TOP/all-conductor flux, electrode balance и areas. Algebraic continuity/electrode/omitted-row error <=`1e-8`; full/reduced observable gate `2%`, absolute current floor `1e-6 A`.

При slot failure continuous не запускать. Не запускать magnetic/domain refinement, hybrid, STM32, direct DSL, profile3 или серию повторов. Прежний magnetic blocker не считать исправленным current PASS.

## Отчёт и остановка

Создать `docs/experimental/analyze/p7-d1-loaded-current-results.md`: родительская коррекция, equations, diff, tests/build/hashes/resources, loaded-state provenance, complex flux tables, full/reduced errors, PASS/NO-GO по каждому gate. Обновить `CHANGELOG.md` при реальном изменении verification workflow. Построить и открыть plots при появлении numerical audits.

Остановиться для родительского ревью. Не коммитить и не начинать hybrid автоматически.
