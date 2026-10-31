# P7-A: статусы проверок и настоящий direct-DSL regression

2026-10-31. Реализация разрешена после родительского ревью. Агент `/root/p7_a_dsl_validation`. Сосед `/root/p7_a_em_accuracy` завершает физические references и scenarios по next-task-p7-a-em-baseline.md. Это параллельная конкретная реализация, не ревью ради слота.

Прочитать AGENTS.md, CHANGELOG, p7-a-em-baseline-results.md и актуальный единый план. Существующие uncommitted изменения сохранять. Production/native/B1 геометрию и зависимости не менять, addon не пересобирать. Не коммитить/пушить, не создавать субагентов.

## Исправить замечания

1. scripts/verify-p7-closure-stripline.ts сейчас напрямую вызывает AnalysisElectricalSession.solveCrossSection. Перевести положительный путь на существующий runAnalysisDsl с настоящим tests/fixtures/analysis-p7-closure/coupled-stripline.js. Сохранить полный report/observables и физическую задачу. Native-only baseline остаётся помеченным историческим reference для regression; не называть его полным DSL timing.
2. Comparator failure должен давать nonzero exit; supervisor отличает numerical/physical validation failure, timeout/resource failure, unsupported и missing evidence. Нулевой exit без нужного machine-readable result не является успехом. Не переименовывать fail в pending или success. --reuse не является cold solve, не присваивать сохранённому результату нынешний addon hash без исходной provenance.
3. Осмысленные tests: принудительное превышение допуска, пропавший результат, несовпадение units/ports/frequency/schema, повреждённые/nonfinite matrix entries, child timeout/nonzero exit. Подтвердить фактические exit/status поведения subprocess, не только compareValue unit-tests. Сделать narrow test seam без запуска тяжёлого solver для каждого negative.
4. Истинный положительный cold direct-DSL run на неизменной coupled-stripline,3 частоты1/10/100MHz, после чего сравнить полные C/L/modes/complex S с сохранённым native baseline. Полное совпадение является regression, не независимой физической точностью; аналитическая TEM identity сохраняет этот же ограниченный статус.
5. Supervised время включает DSL/worker/preparation/validation/serialization до завершённого результата. Сохранить input/build hashes, native timing отдельно, process-tree sampled RSS/private commit с честным peak convention. Сосед может использовать этот supervisor после согласования интерфейса.

## Владение

scripts/verify-p7-closure-stripline.ts, verify-p7-closure-supervised.py, verify-p7-closure-regression.py; tests/helpers/p7-closure-reference.ts; tests/analyze-p7-closure-reference.test.ts и новые tests/analyze-p7-closure-execution.test.ts; coupled-stripline.js только при необходимости исправления исполнения без изменения физики; docs/experimental/analyze/p7-a-dsl-validation-results.md. Не менять manifest, physical baseline runner и другие fixture files — передавать нужные hashes/fields соседу сообщением. Не редактировать отчёт соседа и общий CHANGELOG/roadmap. Проверить shared helper compatibility до изменения API.

## Приёмка и бюджеты

Новые negative tests ловят реальные прежние дефекты; один успешный direct-DSL запуск на stable addon, правильный status и полный response regression; `npm run typecheck`/`npm run build` и focused TS tests по изменениям. Если full response shape отличается от native, сопоставить физические поля явно, не обрезать до одной задержки.

Малый сценарий сохраняет30s/512MiB native budget. Существующий supervisor1.5GiB/145s и host headroom; не повышать его при новом отказе. Один heavy run одновременно с accuracy агентом, без бинарных замен; короткие tests согласовать по нагрузке. Не повторять unchanged failure. Проверка B1 whole geometry85s и большой magnetic не нужны.

Итог: файлы, команды, отрицательные/положительные результаты, hashes, точные cold metrics, ограничения. Никаких claims абсолютной точности C/L/crosstalk по homogeneous velocity identity. Не закрывать весь A/P7 по успеху этого подзадания; остановиться для родительского ревью.
