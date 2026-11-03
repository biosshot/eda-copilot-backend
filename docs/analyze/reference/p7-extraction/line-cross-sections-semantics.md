# P7a: численные сечения идеальных однородных линий

Production путь: существующие `a.line()` → `a.impedance().method("cross-section")`
→ disposable scenario/process → метод `AnalysisElectricalSession.solveCrossSection`
единственного addon → `native/analyze/src/extraction/`. Результат `z.network` —
обычный NetworkResource P6b. Нет второго runtime/addon, GPU, стороннего simulator,
автоматического PCB extraction, cache или общей DC/FEM абстракции.

## Геометрия и физический смысл

Поддержаны microstrip, stripline, coupled-microstrip, coupled-stripline.
Одна/две прямоугольные PEC полосы конечной толщины; у пары одинаковая ширина,
общий сигнальный уровень, постоянный **edge-to-edge** gap. `positive` слева,
`negative` справа; исходный immutable line ID сохранён в solution/segments/network.
Внутренний field kernel возбуждает набор проводников по отдельности и не содержит
эмпирических формул под kind. Перестановка conductors переставляет RHS/поля/матрицы.

DSL задаёт mm; адаптер переводит в m. Ось y сечения направлена **вверх** от нижней
reference, x вправо. Это отдельное сечение, не изменение Y-down координат V2.
`dielectricHeightMm` microstrip — от плоскости до **нижней** поверхности полосы;
сверху воздух εr=1. Для stripline `bottomDielectricHeightMm` — нижний чистый зазор,
`topDielectricHeightMm` — верхний чистый зазор; расстояние между плоскостями равно
bottom + copper thickness + top. Граница двух материалов проходит по нижней
поверхности полос: верхний материал также заполняет боковые зазоры на высоте меди.
Верхняя/нижняя толщины и ε могут различаться.

Каждый материал требует положительную конечную εr ≤100, **явные** `lossTangent:0`
и непустой state. Остальные свойства материала отклоняются: нельзя принять и
игнорировать conductivity, dispersion, permeability, реальные потери. Маска,
ненулевой lossTangent, variants, temperature/physical overrides, numerics overrides,
formula/layered/channel/differential/Resource targets — явный unsupported.
Длина, если задана, строго положительна. Нет длины → погонные C/L/Z доступны,
полная delay unavailable, доступ к network бросает RESULT_UNAVAILABLE; условного
метра нет. Unknown/future targets не становятся результатом с нулевыми числами.

## Сетка, FEM и качество

Согласованная градуированная прямоугольная сетка, каждая диэлектрическая ячейка
разбита на два P1 треугольника. Металлические кромки/границы материалов — точные
линии сетки; внутренность металла исключена. Сгущение около кромок и малого gap;
в отчёте фактические minimum angle/maximum aspect. Тонкие полосы дают анизотропные
прямоугольные треугольники; aspect >1e9 отклоняется, малый angle не скрывается.
Это не готовая DC-сетка внутри меди и не адаптивный full-wave solver.

На идеальных reference φ=0; каждая сигнальная полоса по очереди 1 V при остальных
0 V. На искусственных боковых и открытой верхней границах natural homogeneous
Neumann (нулевой нормальный D), **не заземлённый экран**. Область увеличивается
независимо от сетки. Для stripline верхняя/нижняя плоскости идеальны и соединены
данным условием reference; произвольные реальные PCB grounds не объединяются.

Сборка stiffness масштабирована на ε0. Sparse faer LLT факторизуется один раз
на материализацию C или C0, сразу решаются 1/2 RHS; C0 использует **ту же** сетку
и вакуум. Maxwell C получается из Dirichlet reactions и проверяется интегралом
ε grad φᵢ·grad φⱼ. Диагональ положительна, mutual C отрицательны; abs/projection/
искусственного обнуления связей нет. L определяется решениями C0 L = μ0 ε0 I
(один/два RHS, малое исключение), это **внешняя** L идеальных немагнитных PEC.

Сохранены φ [V], треугольные E=-grad φ [V/m], материалы, BC tags (0 free,
-1 reference, 1..n conductor order), C/C0, энергии J/m, reference charges C/m.
Guard ≤1e-8: row-normalized residual, symmetry/self, energy-vs-reaction/self,
charge imbalance/sum absolute boundary charges. SPD и знак mutual проверяются.
Ни один guard не ремонтирует результат. Переполнение/исчезновение modal Z/delay
или nonfinite S дают SECTION_NONFINITE, а не JSON null или успешный ноль.

При последовательном делении near-edge шага пополам сравниваются все C/C0/L,
Z modes и modal delay/m. Принятие между двумя последними уровнями ≤1%; для weak
mutual denominator floor = 0.001 × наибольший self соответствующей матрицы.
Затем при том же шаге увеличивается artificial padding ×2 (до трёх сравнений,
scale ≤64), требуется тот же критерий. Все уровни/матрицы/размеры сохранены.
Не достигнуты guards/caps → rejected, convergence history остаётся в native
отчёте. DSL возвращает диагностический отказ, выполнение можно продолжить catch.

## Параметры и распространение

Single: Z0=√(L/C), delay/m=√(LC). Pair: eigenvectors [1,1] (even), [1,-1] (odd);
численная reflection symmetry проверяется ≤1e-8/self. `Zdiff=2 Zodd`,
`Zcommon=Zeven/2`; Vdiff=V+−V−, Idiff=(I+−I−)/2,
Vcommon=(V++V−)/2, Icommon=I++I−. Моды не усредняются.

`characteristicImpedance`: single scalar Ohm; pair table Ohm,
`data.rows=[{name:even/odd/differential/common,value}]`.
`delay`: single scalar s; pair table s с even/odd rows, умноженными на длину.
`rlgc`: table unit `1` (структура смешанных единиц): conductorOrder, matrices
capacitanceFPerM/vacuumCapacitanceFPerM/inductanceHPerM, modes, differentialOhm/
commonOhm, units map. resistanceOhmPerM/conductanceSPerM содержат matrix и state
**assumed zero PEC / assumed zero lossless dielectric**; это не вычисленные
нулевые реальные losses. `segments`: один однородный участок с исходным lineId,
размерами, boardIdentity и conditional. Поля/сходимость/assumptions в solution.

Из модальных C/L аналитически строится распределённая lossless линия заданной
длины. Нет lumped ladder и обязательной промежуточной сингулярной Z/Y. Полный
2×2/4×4 S имеет порядок `input,output` либо
`positiveInput,negativeInput,positiveOutput,negativeOutput`; row=out,column=in.
Токи обоих концов внутрь сети, exp(+jωt), matched S21=exp(−jωdelay).
Guard unitarity ≤1e-10, конечность и reciprocity; никакой проекции S.
50 Ohm — normalization power waves, не нагрузка и не рассчитанный Z0.

Одно сечение решается один раз для frequency axis. Частоты только меняют фазу
распространения. Cache отсутствует; разные задачи/длины пересчитывают сечение,
не зависят от скрытой истории. Старые handles/resources не меняются.
Рассчитанный NetworkResource содержит **ровно** запрошенную сетку, validity,
исходный geometry SHA256, assumptions/applicability. P6b complex interpolation
conditional; coarse grid не становится точным повторным решением длинной линии,
экстраполяции нет. Raw Line в connect остаётся unsupported.

## Применимость, caps и исполнение

Режим — lossless quasi-TEM, не реальные потери FR-4/меди. Screening:
max physical transverse dimension × √max ε ≤ λ0/20. Это консервативная
геометрическая **эвристика**, не full-wave proof. Размер artificial domain и длина
линии не участвуют в этом ограничении; электрически длинная линия допустима.
Численная сходимость сама по себе не доказывает применимость. Для всех результатов
validity conditional с точной причиной, для board examples — ещё stackup/reference
hypothesis; никто не объявляет их измеренным PCB channel.

- Default maxNodes 200000/maxTriangles 400000, hard 300000/600000; 1/2 conductors
  (до двух RHS на факторизацию), maxRefinements default/hard 5; одна задача за раз.
- Near-edge default step=min(width,clear reference heights,gap)/6; maxEdgeMm
  позволяет явно задать начальный шаг. Domain scale default 8, допустимый initial
  4..32; maximum comparison scale 64. Axis recursion ≤30, около 2000 points/axis.
- Размеры DSL >0..10000 mm, основные features ≥1e-7 mm; extreme aspect ratios
  и gap/width <1e-5 отклоняются. Это caps поддержанного метода, не CAD repair.
- До mesh allocation проверяются верхние bounds node/triangle counts и план RAM:
  2048 × grid nodes + 512 × triangle bound + 64 KiB. Перед numeric LLT добавляются точные symbolic factor values/workspace sizes;
  он учитывает массивы/RHS, sparse work и предыдущий retained field, но **не является
  доказанным жёстким OS allocator/RSS cap**. Default planned budget 768 MiB;
  явный maxMemoryMiB 1..4096. Фактический process high-water RSS записывается.
- ≤8192 frequencies, F×(2n)² ≤131072 output coefficients; JS проверяет до Array.from,
  Rust повторяет admission. Phase argument ≤1e9 radians. Нет fan-out frequency×mesh.
- Native maxSeconds default 120; проверки между крупными операциями. Отмена/deadline
  синхронной факторизации — прежнее завершение disposable process, не кооперативная
  гарантия прерывания внутри LLT. Нет новых queue/worker/cache механизмов.
- Старый default report budget 4 MiB сохранён. Подробные большие field reports
  требуют явного `maxOutputBytes` (named fixtures используют ограниченные 128 MiB).

## Независимые эталоны и примеры

Versioned [references.json](../../../../tests/fixtures/analysis-cross-section/references.json)
содержит геометрию, единицы, диапазоны/метод воспроизведения и численные значения.
Qucs formulas используются **только как независимые инженерные test references**,
не production solver. Thin-limit отделён от finite-thickness сравнения.

- [Qucs single microstrip](https://qucs.github.io/tech/node75.html): Hammerstad/Jensen
  static + thickness; [coupled](https://qucs.github.io/tech/node77.html): статические
  Kirschning/Jansen even/odd. Проверяется 3% на перечисленных случаях, не universal PCB accuracy.
- [Ansys stripline table](https://ansyshelp.ansys.com/public/Views/Secured/Electronics/v252/en/Subsystems/Circuit/Content/Circuit/TRLStriplineTransmissionLine.htm):
  b=1.270 mm, t=.010 mm, ε=2.2, widths .5/1/2 mm, Z=74.86/50.69/30.93 Ohm.
  Используется Z геометрии, не приведённые в статье loss coefficients.
- Аналитические single/multilayer parallel plates с natural side BC, одинаковый
  production field kernel; C/energy/charge, mm→m. Air LC и матричный C0L — дополнительные инварианты.
- Независимая сумма многократных отражений проверяет complex S с допуском
  1e-8 relative +1e-10 absolute, включая matched/quarter/half wave, длины, пару.

Три прямых `impedance.js`/тонких launchers: BandAmpV5 RF_IN_PORT, STM32 SWCLK/SWDIO
(реально параллельные соседние линии, **не объявлены differential bus**), ThunderF722
UART5_RX. Audit сверяет V2 IDs/net/layer/полные source tracks и выделенный общий
отрезок/ширину/gap/длину; проверяет поле, E из φ, интеграл C, C0L, единицы/моды,
grid/ports, complex oracle, plot arrays, fail→continue, conditional и hashes.
Negative audit mutations не должны проходить. HTML self-contained, исходные
численные массивы сохранены отдельно; canvas показывает реальную сетку/φ/E.

За пределами: finite ground/copper losses/roughness/skin/proximity, lossy/dispersive
materials, via/plane gaps, arbitrary PCB channel/PDN, full-wave/radiation, fitting,
FFT/TDR/time/SI/eye/BER, F32/GPU. Полный P6/P7/P11 этим не закрываются.
