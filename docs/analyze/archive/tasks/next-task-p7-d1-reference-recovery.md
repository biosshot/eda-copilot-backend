# P7-D1R: joint-reference current recovery и допустимость эталона

2026-10-31. Один исполнитель. Родительское ревью подтвердило D1 NO-GO и воспроизвело reduced replay на final addon. Это отдельная joint-only диагностика, не продолжение hybrid и не новый этап D/E/F.

## Контекст и исходные факты

Прочитать `AGENTS.md`, `CHANGELOG.md`, `chat-handoff-2026-10-31.md`, `next-task-p7-d1-coupled-assembly.md`, `p7-d1-coupled-assembly-results.md` и актуальный план `p7-plan.md`.

Checkpoint всей предыдущей работы: `db535f0`. Не reset/clean и не удалять пользовательские/сохранённые данные, особенно `native/analyze/debugging/` и `debugging/`. Не создавать субагентов, не коммитить и не пушить.

D1 уже установил:

- final addon SHA256 `f15e8aee3c87da0847902c571199a9bc2eaa5f116bf3bb957f686fb9d727ee73`;
- joint magnetic patch1→2: `10.2046289%` > 2%; domain2→4mm: `0.1116198%` проходит только эту численную проверку;
- slot joint recovered return flux: `-0.4162806829 A` вместо electrode balance `-0.9995544315 A` на сечении `x=1.000123 mm`;
- continuous control: flux `-0.9995649933 A`, electrode balance `-0.9995656895 A`, площадь сечения совпадает;
- recovered slot cross-section area `1.9000000507e-9 m²` соответствует двум реальным обходам;
- current-basis R-orthogonality и reconstruction проверены на уровне сохранённого recovered state; это не доказывает физическую непрерывность тока;
- matched-load S/energy transport исправлен и родительским reduced replay воспроизведён. Его заново не проектировать.

Ключевые входы и evidence:

- fixed geometry `tests/fixtures/analysis-p7-joint-section/slot.json`;
- continuous control `tests/fixtures/analysis-p7-joint-section/continuous.json`;
- joint packet `debugging/p7-closure/1793437157149686200/`;
- continuous packet `debugging/p7-closure/1793438893314147000/`;
- replay/figures `debugging/p7-d1-replay-final/`;
- current recovery `tests/fixtures/analysis-p7-joint-section/recover-magnetic.ts`;
- flux integration `tests/fixtures/analysis-p7-joint-section/return-flux.py`;
- minimal reduced replay `tests/fixtures/analysis-p7-joint-section/replay-reduced.ts`.

## Цель

Ответить на один вопрос: почему fixed slot joint candidate не даёт физически согласованного возвратного тока через обходы и можно ли после этого считать его численным reference?

Нужно разделить по доказательствам:

1. ошибку исходной full electric/magnetic current постановки или решения;
2. потерю/искажение при reduction и последующем recovery;
3. ошибку postprocessing: source→cell mapping, cell density, площадь/ориентация сечения, комплексная фаза, единицы;
4. фактическую неразрешённость mesh/current space вокруг щели.

Нельзя автоматически объявлять причиной scalar section/local interface: hybrid в этом задании не изменяется и не используется для доказательства причины.

## Обязательная последовательность

### 1. Сначала saved evidence, без нового FEM

Проверить и записать в новый отчёт точную формулу electrode balance, знаки, real/imag convention и единицы. Убедиться, что continuous control действительно валидирует тот же integrator, а slot использует ту же физическую нормаль и полный набор BOTTOM source cells.

Используя сохранённые prepared/model/recovery данные, насколько они это позволяют:

- сравнить full/recovered current state с reduced reconstruction на одинаковом возбуждении;
- проверить комплексный, а не только real, flux и electrode balance на 1 kHz;
- вычислить flux на нескольких уже существующих tetrahedral сечениях до щели, внутри обходов и после щели;
- проверить сумму тока по всем физическим проводникам сечения, не только возврат;
- сопоставить recovered constant-vector current с доступным conduction current из electric state (`sigma E`), не создавая новый solver.

Если сохранённых состояний недостаточно, явно назвать недостающее поле и только тогда подготовить минимальный joint-only rerun.

### 2. Локализация в native

Найти существующие current-space construction, reduction и recovery. Не переписывать метод. Для fixed slot candidate определить:

- сохраняются ли обе медные ветви обхода в current cells/source mapping;
- существуют ли current DOFs/basis functions, способные представить разные токи в двух ветвях и продольное перераспределение около щели;
- выполняется ли локальная/интегральная continuity не только в слабом глобальном residual, но и на физических сечениях;
- одинаково ли обрабатываются slot и continuous, кроме самой геометрии/mesh.

Гипотезы проверять отдельными маленькими controls. Не добавлять trace moments, глобальную plane L/C, эмпирический масштабный коэффициент или новый публичный selector.

### 3. Минимальная правка только при доказанной причине

Если найден конкретный реализуемый дефект текущего пути, разрешена минимальная правка в существующем `native/analyze/src/extraction/local/` и focused tests. До правки записать в отчёте причину, ожидаемый эффект и почему она не меняет физические guards/допуски.

Если причина не установлена или требуется другой field solver/current formulation — остановиться с NO-GO и точным blocker. Не начинать перебор методов, mesh profile3, F32/GPU, out-of-core, OS paging или повышение лимитов.

## Повторные вычисления и ресурсы

Сначала replay/reused evidence. Не повторять unchanged failures и не запускать автоматическую серию уточнений.

Если изменился native current path, допустим один bounded before/after joint-only контроль на fixed slot и один continuous control. Один heavy run одновременно. Общий ориентир каждой попытки <=300 s и <=2 GiB private/RSS с host headroom; не использовать диагностические 8 GiB/600 s. Сохранить холодные identity/code/hash/resource records. Никаких полных STM32 и direct-DSL кампаний.

При native изменении выполнить штатный `npm run native:build`, новый addon SHA256 и focused Rust tests. Выполнить соответствующие TS tests, typecheck; build при изменении публичного/TS пути. Не выдавать старый cache за результат нового addon: provenance/core identity должны быть явными.

## Приёмка результата задания

Положительный результат этого этапа означает только допустимый joint reference candidate:

- объяснена и устранена либо точно локализована потеря тока;
- complex return/electrode balance согласованы на проверенных сечениях с заранее записанными floors;
- continuous control сохранён;
- magnetic patch1→2 и domain sensitivity заново измерены только при изменившемся численном пути и проходят существующие 2% gates;
- нет ослабления guards, изменения геометрии или подгонки коэффициентов.

Даже при PASS это не D1/P7 acceptance: hybrid comparison выполняется отдельным последующим заданием после родительского ревью.

Если reference остаётся недопустимым, результат NO-GO должен содержать минимальный воспроизводимый дефект и следующую необходимую математическую/implementation работу, без вывода о невозможности hybrid в целом.

## Отчёт и остановка

Создать `docs/experimental/analyze/p7-d1-reference-recovery-results.md`: постановка, использованные/новые пакеты, команды и hashes, real/imag flux tables, локализованная причина, diff/правки, tests/resources, PASS/NO-GO и ограничения. Новые локальные artifacts и графики — в `debugging/p7-d1-reference-recovery/`; графики открыть. Обновить `CHANGELOG.md` только при реальном изменении поведения/verification workflow.

Не менять статус общего плана и не объявлять D1/P7/P8/P11 закрытыми. После результата остановиться для родительского ревью.
