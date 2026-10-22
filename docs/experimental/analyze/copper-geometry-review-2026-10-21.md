# Независимое ревью геометрии меди

Историческое ревью. **R1 и R2 исправлены и приняты 2026-10-22:** [повторная проверка](copper-geometry-review-2026-10-22.md). Ниже сохранены исходные замечания и воспроизведение.

Дата: 2026-10-21. Проверены рабочие изменения после `bf3869c` по заданию `next-task-copper-geometry.md`.

**Решение: этап пока не принимается, коммит не сделан.** Найдены воспроизводимая ошибка физической связности и регрессия существующего теста addon. До исправления не переходить к meshing/DC.

## R1 — P1: аппроксимация drill создаёт подтверждённый ложный путь внутри одного источника

Место: `native/analyze/src/geometry/mod.rs`, вычитание drills и создание `Copper` (строки 622–645 на момент ревью); `topology.rs` считает такой полигон одним узлом графа. Погрешность проверяется при контактах **между** регионами, но не при установлении связности **внутри** получившегося региона.

Независимый аналитический купон:

- прямоугольная медь 6 × 1.999 mm, центр (0, 0), поворот −87.5°;
- круглое NPTH диаметром 2 mm в центре, сквозной вырез;
- площадки A и B размером 1 × 1 mm расположены вдоль длинной оси полосы на расстояниях −2 и +2 mm;
- исходная полоса без net; ожидается, что она не создаёт путь A ↔ B.

Истинная окружность пересекает обе длинные кромки полосы, поэтому медь разрезана на две части. При default deviation 0.001 mm отверстие представлено вписанным 72-угольником. Поперечная ось полосы проходит посередине между его вершинами: максимальный вылет многоугольника равен `cos(π/72) ≈ 0.99904822 mm`, а половина ширины полосы — `0.9995 mm`. Аппроксимация оставляет две ложные тонкие перемычки.

Фактически текущая реализация возвращает:

| Погрешность | Фрагменты полосы | Подтверждённые островки | Результат |
| --- | ---: | ---: | --- |
| 0.001 mm | 1 | 1, с nets A и B | `complete`, diagnostics пусты, `physical_short` |
| 0.0001 mm | 2 | 2, A и B раздельно | `complete`, без short |
| 0.00001 mm | 2 | 2, A и B раздельно | `complete`, без short |

Добавлен независимый тест `drill_approximation_cannot_create_a_reliable_path_inside_one_source` в `native/analyze/tests/geometry.rs`. На 0.0001 mm проверка проходит; на 0.001 mm падает. Это ошибка модели, а не спор о последнем разряде площади: ложный путь попадает в надёжный граф и может дать неверное решение будущего DC.

Команда воспроизведения:

```sh
cargo test --release --locked --manifest-path native/analyze/Cargo.toml --test geometry drill_approximation_cannot_create_a_reliable_path_inside_one_source
```

Отдельный Node-reproducer и JSON/полные отчёты сохранены локально в игнорируемом `debugging/review-copper/`. Тест Rust самодостаточен и не зависит от этих файлов. В ходе ревью production-код не исправлялся.

## R2 — P2: существующий тест списка native exports падает

В `tests/pcb-native-addon.test.ts:44` ожидается ровно прежний список плюс `importAnalysisBoard`. Новый `prepareAnalysisGeometry` отсутствует в expected, поэтому текущий штатный тест завершается с ошибкой (1 failed, 4 passed).

```sh
node --import tsx --test tests/pcb-native-addon.test.ts
```

Это не повод ослаблять проверку: нужно сохранить полный прежний список и явно добавить один согласованный новый export, вместе с проверкой declarations. Проверку не включили в список выполненных focused tests реализации.

## Что независимо выполнено

- `npm run native:build` — PASS, проверен свежий addon.
- `cargo test --release --locked --manifest-path native/analyze/Cargo.toml` **до добавления review regression** — 17 geometry + 13 import passed, включая корпус восьми плат; geometry suite около 80 s.
- Новый аналитический review regression — FAILED, воспроизводит R1.
- `npm run typecheck`, `npm run build` — PASS.
- `node --import tsx --test --test-concurrency=1 tests/pcb-analysis-geometry.test.ts tests/pcb-analysis-runtime.test.ts` — 21 passed, 0 skipped; placement backend selectors CPU.
- Существующий `tests/pcb-native-addon.test.ts` — 4 passed, 1 failed, R2.

Логи: игнорируемые `.artifacts/review-geometry-*.log`. Повторный package smoke, полный npm test и повторный Node-прогон восьми geometry fixtures не запускались после выявления блокера; сохранённые свидетельства автора не выдаются за независимое прохождение этих проверок. Linux/macOS и реальный GPU не проверены.

Следующее задание — [исправить достоверность связности и завершить приёмку](next-task-copper-geometry-corrections.md). Исходный геометрический этап остаётся открытым.
