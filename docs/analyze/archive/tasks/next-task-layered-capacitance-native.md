# P7: внутреннее Rust-ядро проверенной layered capacitance

2026-10-24. Одобрен переход от проверочного скрипта к ограниченной native
реализации. Основание — §8.15 и native-port proposal в worktree P7.
Это не публичный анализатор и не закрытие P7.

## Принятое доказательство

Parent независимо сверил37 source/input/output hashes, пересчитал full quadratic
errors из raw FastCap reference и просмотрел PNG. D64 domain-only4.77665%,
mesh-only4.41974% <=5%; отдельные domain/mesh changes0.78250/0.60540% <=2%.
Reference/geometry: finite vacuum two plates1mm square,35um foil,965um gap.
Final diagnostic40917nodes,2.79648Mterms,10.16s,375.23MB; не arbitrary-PCB budget.
Только этот coupon принят; все earlier failures сохраняются.

## Реализовать до законченного пакета

1. В существующем extraction/local сделать CPU/F64 P1(XY)*P1(z) assembly:
   conservative overlaps, degree2 XY moments, analytic z integrals, finite
   copper exclusion и полный face/sidewall conductor trace. Переиспользовать
   pinned Spade CDT, geometry/source identities и faer; не вводить отдельный
   service, общий compute runtime или Python runtime dependency.
2. Exact-mesh adapter для воспроизводимого frozen coupon и обычный путь через
   native CDT. Не объявлять прежний tetrahedral primitive эквивалентным
   product space. Не наследовать alignment/drop policies, теряющие source
   constraints. Unsupported overlaps/ownership/materials дают отказ.
3. Typed sparse assembly с actual symbolic admission и полным simultaneous
   memory plan, включая geometry/COO/CSC/factor/scratch/RHS/output. Одно время
   на весь request, без свежего budget для подэтапов. Defaults существующего
   production API не увеличивать. Внутренние coupon requests могут явно задать
   diagnostic50k/6M/512MiB/60s как в принятом эксперименте.
4. Полная C по всем conductor potentials, charges, поле и U=.5vTCv;
   infinity truncation явная, ни один conductor не hidden ground.
   Никакого второго P6 time solver и никаких правок его shared types без
   согласования с владельцем P6.

## Приёмка

- Перенести минимальные воспроизводимые inputs/generator/reference в tracked
  fixtures, не гигантские debug outputs. Сохранить failed D32 и accepted D64.
  Exact same mesh native/Python full quadratic parity <=1e-10; отдельно
  independent full accuracy<=5% и раздельные domain/mesh changes<=2%.
  Для другой CDT triangulation проверять внешнюю accuracy заново.
- Независимые unit/patch tests для piecewise epsilon и cross-layer overlap,
  constant/affine fields, asymmetric material placement, full matrix energy.
  Добавить finite shifted-foil и three-conductor coupons с независимыми
  references. Для каждого source/reference/domain/mesh uncertainty явно
  отделена от algebraic residual. Reciprocity/charge/work/recovery gates
  заранее записать; не менять допуски после failures.
- Dielectric full-matrix reference брать из первичного solver/аналитики,
  поддерживающей данный dielectric, не vacuum-only FastCap по ошибке.
  Внешний solver — только offline reference, не runtime/CI dependency.
- Vias остаются unsupported до отдельных finite barrel/span/bore/interface
  coupons; их реализацию сейчас не расширять. Real-board integration/DSL,
  broad ROI, L kernel/cost и magnetic energy остаются следующим этапом.
- Focused native regressions, allocation/deadline/geometry negatives,
  numerical PNG из saved values, manifest и фактические memory/time.
  Новая additive библиотека без public API не требует повторять весь corpus.
  Если нужен build общего addon, сохранить default GPU placement, jobs1.
- Выдать готовый review packet либо конкретный numerical blocker; не
  останавливаться на ещё одном плане. Не коммитить, новых агентов/этапов нет.
  Heavy slot согласовать с Thermal/P6. После пакета остановиться.
