# D1 / P7: handoff для продолжения после остановки 2026-11-02

## 1. Текущее состояние

Поручение: **«закрывай D. Сборка канала полностью и до конца!»**.

**D1 и P7 пока НЕ закрыты.** Остановка вызвана лимитом текущей сессии, а не завершением физических проверок и не новым численным отказом.

Рабочая папка: `D:\MyProject\NN\projects\Agents\easyeda-copilot\eda-copilot-backend`.

Ветка: `feat/analyze`; исходный HEAD этой работы: `c2fef93`.

Вся незакоммиченная работа сохранена. Ничего не откатывалось, не удалялось и не отправлялось в remote. Коммит не создавался. Папка `debugging/` со всеми артефактами сохранена.

Субагентов не запускать. Одновременно выполнять только одну тяжёлую сборку или численный прогон. Для Rust использовать:

```bash
PATH="/c/Users/kiril/.cargo/bin:$PATH"
```

## 2. Что сделано в этой работе

### Conservative current и AC/DC

- Для обоих фиксированных каналов D1 реализованы полные и reduced loaded-state recovery, корректное AC superposition и сохранение полной DC-матрицы для независимого replay.
- Исправлены structural gauge, electrode/cell constraints, loaded DC gauge, AC RHS orientation, compensated loaded solves и DC terminal flux ownership.
- Эти изменения уже дали прохождение current gates на холодных slot и continuous controls. Это не означает закрытие остальных D1 gates.

### Magnetic energy и полный канал

- Создан полный loaded reference: `debugging/p7-d1-loaded-current/slot-ac-final-6-full.json`, 186 cells.
- Получено согласование direct full mutual и joint network:
  - direct full mutual: `2.6782987350311896e-11 J`;
  - joint network: `2.678298735013973e-11 J`.
- Section direct self подключён через `channel_magnetic.rs` и `segmented-channel.ts`.
- Local FEM self согласован с direct local integral.
- Training в `board_prepare.rs` окончательно `[1e4, 1e5]`.
- Настроена automatic channel mesh:
  - стартовая сетка не крупнее `5e-8`;
  - local level 1/2: `5e-8` / `1.25e-8`;
  - section level 1/2: `1e-8` / `5e-9`.
- Executor использует coarse/fine только для domain1; domain2/4 — coarse. Final fine domain1 имеет `boundaryFraction=1/3`, shifted coarse — `1/4`.

### Точное повторное использование magnetic kernels и моделей

- `native/analyze/src/extraction/local/magnetic_kernel_cache.rs`: process-local geometry-only cache, 16 MiB / 24 entries, canonical vertex/cell ordering, IEEE-биты/order/tolerance в ключе, отдельные square/pair kernels и восстановление исходного порядка.
- `volume_integral_magnetic.rs` использует square kernel с tolerance `1e-10`; `channel_magnetic.rs` — square/pair с `1e-12`. Near path остаётся `tetra_integral_adaptive`.
- `execute-problem.ts`: persistent full `channel-magnetic` cache через `LocalModelCache`, identity `channel-magnetic-v1` + binary core + полный physical request. Budget не входит в identity, но проверяется при restore.
- `board_mesh::Request` получил `coordinate_origin_mm: Option<[f64;2]>`; automatic local blocks передают `coordinateOriginMm: base2.roiMm[0]`.
- Добавлен тест cold → memory → changed budget → fresh-process disk restore в `tests/analyze-local-cache.test.ts`.

### Отклонённый analytic separated-path experiment

- `native/analyze/examples/d1_analytic_far_profile.rs` сохранён как diagnostic, но производственный путь не менял.
- Результат: FAIL — max reference error `2.1940760691941254e-10 > 1e-10`, max analytic/direct difference `7.078910252495783e-8`.
- Время: direct `49.003 s`, analytic `45.474 s`.
- Не возвращаться к этому варианту без нового обоснования.

## 3. Последний полный DSL до текущего electric-domain patch

`slot-real-dsl-kernel-cache-2` запускался без пересборки `dist`, поэтому не является результатом актуального кода и не должен использоваться как доказательство.

После `npm run build` был запущен genuinely cold:

```bash
python debugging/p7-d1-cross-coupling/run_bounded.py slot-real-dsl-kernel-cache-3 dsl
```

Результат:

- wall time `292.125 s`;
- peak RSS `134303744 B`;
- peak private `84865024 B`;
- supervisor status `complete`;
- **`dsl.json.status = failed`**.

Supervisor exit 0 сам по себе не означает принятие D1.

Warm replay через diagnostic script сохранил `replay-error.json`. Старый magnetic kernel/domain issue был устранён, но открылись electric/wave/delay gates:

- domain1 spatial max `0.00007387335392250281` — PASS 2%;
- domain1→2:
  - magnetic `2.5021846108254928e-12` — PASS;
  - wave `0.000015444278777296085`;
  - copper loss `1.3260409416411385e-11`;
  - group delay `0.13739995262836868` — FAIL;
- domain2→4:
  - magnetic `2.2712793656360247e-12` — PASS;
  - wave `0.000022691528749983942`;
  - copper loss `1.139274611819548e-11`;
  - group delay `0.1679573848596931` — FAIL.

Сохранённые delay:

- domain1: `1.5436896725613556e-11 s`;
- domain2: `1.7895775378265608e-11 s`;
- domain4: `2.1508243698055773e-11 s`.

Артефакты находятся в `debugging/p7-d1-cross-coupling/slot-real-dsl-kernel-cache-3/`, включая:

- `replay-error.json`;
- `domain-1-diagnostic.json`, `domain-2-diagnostic.json`, `domain-4-diagnostic.json`;
- `domain-diagnostic.log`;
- `domain-plan-geometry.json`;
- `local-domain-delay.json`.

Диагноз: состав меди на domain1/2/4/8 одинаков, но менялась фаза local-блоков. `accepted_electric_request` принудительно оставлял `air_margin_m = 0.008`, тогда как domainScale расширял ROI/corridors и section air. Поэтому local electric exterior фактически не получал независимого refinement 8 → 16 → 32 мм.

Попытка domain8 закончилась через `62.863 s` ошибкой:

```text
CHANNEL_COMPOSITION: Section run-0 refused: SECTION_MESH_LIMIT: polygon refinement cap
```

Domain8 нельзя использовать как численное доказательство. Не расширять кампанию вслепую и не увеличивать cap.

## 4. Незавершённый electric-domain patch

В исходниках уже добавлен отдельный exterior для electric field, не меняющий current/magnetic ROI:

1. `native/analyze/src/extraction/local/board_mesh.rs`:

```rust
#[serde(default,skip_serializing_if="Option::is_none")]
pub electric_air_margin_m:Option<f64>,
```

Добавлена validation положительного finite значения.

2. `native/analyze/src/extraction/local/board_prepare.rs`:

```rust
field.air_margin_m=r.electric_air_margin_m.unwrap_or(0.008);
```

Default остаётся прежним.

3. `src/analyze/board-channel-plan.ts` automatic local geometry получает:

```ts
electricAirMarginM: 0.008 * (options.domainScale ?? 1)
```

4. `tests/analyze-board-channel-plan.test.ts` содержит проверки `.008` coarse и `.016` expanded.

5. `native/analyze/tests/board_prepare.rs` содержит проверку override. В самом конце работы исправлены две синтаксические ошибки `.016` → `0.016`.

После этого исправления выполнено:

```bash
PATH="/c/Users/kiril/.cargo/bin:$PATH" cargo test --manifest-path native/analyze/Cargo.toml --test board_prepare -- --test-threads=1
```

**PASS: 6/6**, `0.01 s` выполнения тестов.

Затем запущен:

```bash
PATH="/c/Users/kiril/.cargo/bin:$PATH" npm run native:build
```

Сборка дошла до `Compiling eda-copilot-node v0.1.0` и была остановлена пользователем через `222.641 s` из-за лимита сессии. Это **не доказанный build failure**. Ошибок компиляции нового patch в выводе не было; присутствовали прежние warnings. Проверка процессов `cargo`/`rustc` после остановки не нашла процессов (PowerShell `Get-Process` завершился кодом 1 при пустом результате).

**Важно:** текущие native addon и `dist` ещё не содержат подтверждённо собранный `electricAirMarginM` patch. До двух сборок новый DSL запускать нельзя.

## 5. Что было проверено ранее и что нужно повторить

До последних electric-domain изменений успешно выполнялись:

```bash
PATH="/c/Users/kiril/.cargo/bin:$PATH" npm run native:build
npm run typecheck
node --import tsx --test tests/analyze-local-cache.test.ts tests/analyze-board-channel-plan.test.ts
npm run build
```

Тогда: native build PASS (`163.679 s`), typecheck PASS, 21/21 TS tests PASS, build PASS.

После последнего patch эти результаты нельзя автоматически переносить. Нужны новые сборки и focused tests.

## 6. Точный следующий шаг

Из корня `eda-copilot-backend`, последовательно, без параллельных численных прогонов:

```bash
PATH="/c/Users/kiril/.cargo/bin:$PATH" npm run native:build
npm run typecheck
node --import tsx --test tests/analyze-local-cache.test.ts tests/analyze-board-channel-plan.test.ts
npm run build
```

Если native build снова останавливается внешним лимитом на финальном `eda-copilot-node`, сначала проверить отсутствие cargo/rustc workers, затем повторить incremental build. Не считать остановку численным или компиляционным отказом.

После успешного `npm run build` выполнить новый genuinely cold slot в новом attempt:

```bash
python debugging/p7-d1-cross-coupling/run_bounded.py slot-real-dsl-electric-domain-1 dsl
```

Проверить:

- `dsl.json.status == "complete"`, а не только supervisor exit;
- все mesh/domain/cut/joint verification gates;
- wall time ≤ 300 s, память ≤ 2 GiB, один поток;
- host reserve 1 GiB;
- отсутствие orphan workers.

Если domain gate снова не проходит, извлечь полный verification существующим `replay_board_channel.ts` с новым cache. Не ослаблять group-delay gate и не добавлять domain8 без отдельного решения `SECTION_MESH_LIMIT`.

Если slot проходит, выполнить genuinely cold continuous:

```bash
P7_D1_VARIANT=continuous python debugging/p7-d1-cross-coupling/run_bounded.py continuous-real-dsl-electric-domain-1 dsl
```

Затем:

1. Проверить warm/restart на том же binary: checksums, core identity, `assembly=0`, `training=0`, отсутствие orphan workers. Для реального DSL warm/restart можно использовать новые attempts с `P7_D1_RESUME_CACHE=<cold cache>`; supervisor помечает их как verified resume.
2. Поскольку менялись coordinate origin, kernel ordering и electric exterior, повторить актуальные mesh/domain/cut/joint controls на текущем producer. Старые числа нельзя выдавать как результаты нового кода.
3. Выполнить независимый NumPy PSD/passivity анализ final assembled L.
4. Обновить `docs/experimental/analyze/p7-d1-magnetic-closure-results.md`, `docs/experimental/analyze/p7-d1-coupled-assembly-results.md` и `CHANGELOG.md` фактическими результатами.
5. Включить в документацию отклонённый analytic experiment, exact geometry cache, обязательность rebuild `dist`, холодные ресурсы и electric-domain evidence. Не объявлять pass без evidence.
6. В конце выполнить:

```bash
PATH="/c/Users/kiril/.cargo/bin:$PATH" cargo test --manifest-path native/analyze/Cargo.toml --lib -- --test-threads=1
npm run typecheck
PATH="/c/Users/kiril/.cargo/bin:$PATH" npm run native:build
npm run build
```

Коммитить только если действительно закрыты cold slot + continuous + warm/restart + physical gates + актуальные controls. Использовать Conventional Commit, включить только проверенный scope, без push.

## 7. Неизменные ограничения

- Не ослаблять 2% mesh/domain/cut и 8% joint.
- Не увеличивать 300 s, 2 GiB, один поток и host reserve 1 GiB.
- Не подгонять mutual масштабированием или выбором конфигурации по ответу.
- Reduced-модель не считать физическим эталоном без прямой проверки.
- Не вводить задним числом electric-energy acceptance gate; residual electric sensitivity документировать.
- Пользователь допускает близкие числа с предупреждением вне строгой области, но обязательные D1 gates остаются без изменений.
- `npm run build` обязателен перед полным DSL: `runAnalysisDsl` использует `dist/run-analysis.worker.js`, а не актуальный TS source напрямую.
- Сохранять все незакоммиченные изменения; запрещены reset/clean/push и удаление `debugging/`.

## 8. Сохранённый список изменений

`git status --short` на момент остановки содержит изменённые analyzer/native/DSL/test файлы и новые D1 diagnostics/docs. В частности, добавлены:

- `native/analyze/src/extraction/channel_magnetic.rs`;
- `native/analyze/src/extraction/local/magnetic_kernel_cache.rs`;
- `native/analyze/src/extraction/local/volume_integral_magnetic.rs`;
- `native/analyze/examples/d1_*.rs`;
- `docs/experimental/analyze/p7-d1-magnetic-closure-results.md`.

Полный актуальный список восстановить командой `git status --short`; не использовать этот handoff как основание для выборочного удаления файлов.
