# Продолжение P7: модели переходов, каналов и PDN из PCB

Дата: 2026-10-23. База: 096d96e, принятые P7a/P7b. Выполнять вместе с [границами P6/P7](../design/parallel-p6-p7-execution.md). Направление продолжает исходный P7 и подготавливает P8/P11; одна принятая часть не означает закрытие всего P7.

## 1. Цель и последовательность

Следующий основной блок — квазистатическое электрическое извлечение реальной геометрии: локальные переходы/возвратные пути и низко-/среднечастотная модель питания. Использовать её как основу PDN и необходимых неоднородностей канала. После приёмки этого блока продолжить оставшиеся части P7 по исходной матрице, а не самостоятельно переходить к P8/P11.

До production реализации записать конкретную область первой законченной модели, методы, источники, допуски и ограничения. Согласованные направления PEEC/BEM — кандидаты, а не приказ копировать чужой solver или обещать произвольную PCB. Если выбранный метод не покрывает существенный пункт, показать, каким следующим подэтапом P7 он закрывается.

## 2. Физическая постановка и обязательства

- Источник истины — физическая медь, реальные CAD fills, отверстия, pad/barrel геометрия и span, source IDs и явные stackup/materials. RoutingBoardV2 остаётся транспортом; solver использует internal representation.
- Модели учитывают R, собственные и взаимные L/C там, где они физически нужны; reference/возврат — часть постановки. Net name не заменяет геометрическую связность или идеальную землю.
- Нужен корректный DC/низкочастотный предел и явная полоса квазистатической применимости; фиксированный R/L/C не объявляется широкополосной моделью произвольной PDN. Skin/proximity/dielectric эффекты включать только подтверждённым методом либо явно ограничивать диапазон, не применять high-frequency P7b вне его области.
- Существующую DC проводимость использовать/сравнивать там, где это математически корректно. Не добавлять произвольные L/C к DC графу и не считать этим завершённое извлечение.
- Локальные via/соединения: finite barrel, реальные контакты, возвратные переходы, pad/antipad/плоскости и неизвестные данные. Недостающий padstack или return нельзя заменять идеальным элементом без явного авторского допущения и поддержки такого режима.
- Поддержанные слоистые диэлектрики, конечная медь и граничные условия должны соответствовать заявленной модели. Размер области/сеточный error проверяются независимо; взаимность, PSD/passivity и сохранение энергии не заменяют независимую точность.
- Модель должна иметь source/port mapping и достаточные данные восстановления внутренних токов/потерь для заявленных результатов. Если применяется сокращение, отдельно проверить порты и внутренние наблюдения, error в полосе и сохранение пассивности.

## 3. Стык и ownership

P7 владеет native/analyze/src/extraction и новыми узкими geometry/mapping helpers, extraction TS adapter, своими сценариями/эталонами. P6 владеет общим электрическим исполнителем и представлением coupled/dynamic модели. До нового общего DTO согласовать минимальный пример с архитектором и P6.

Предпочитать существующий NetworkResource/S/Z/Y для частотных наблюдений, существующие circuit-описания там, где они полностью выражают физику. Это не требование втиснуть взаимные связи в независимые RLC и не разрешение выдавать finite-band samples за готовую transient-модель. Нужные дополнительные внутренние формы предложить в design addendum.

Сохранить a.channel/a.differential/ports/stackup/resources и уже принятый DSL. Не создавать новый analyzer лишь для обхода schema. Если фактический контракт не выражает вход/результат извлечения, показать минимальный стык до его реализации.

## 4. Границы

- CPU/F64, собственная физическая реализация, существующие math libraries/addon/worker. GPU/placement не менять.
- Нет full-wave, radiation, EMI, Thermal, TDR/eye/timing, nonlinear converters, общего SPICE, автоматического decap placement, solve/optimize.
- В этом поручении не запускать полноценный P8: его источники/нагрузки/transient-проверки всей PDN будут использовать принятую модель следующего этапа. Экстракционные port-response тесты здесь обязательны.
- Не изменять сохранённые платы/заливки для прохождения теста; неподдержанный случай остаётся в evidence с причиной. Экспорт/RawPCB/router не рефакторить.
- Не добавлять fallback-refill как скрытое исправление геометрии. Его исходные обязательства сохраняются отдельным незавершённым пунктом P4.

## 5. Проверки и критерии приёмки следующего законченного блока

1. Независимые аналитические/опубликованные/внешние сохранённые references для R/L/C и полного портового отклика, включая mutual terms; источники и пределы применимости, единицы, геометрия, неопределённость и допуски фиксируются до solver.
2. Минимальный набор: однородная полоса/DC limit; физический токовый контур с явным возвратом; проводники с известной ёмкостной/индуктивной связью; via с возвратным via; plane/PDN coupon; полезный неоднородный/многослойный случай в реально поддержанной области. Отдельно отрицательные неподдержанные spans/возвраты/параметры.
3. Сеточная/областная/частотная сходимость; translation/rotation и перестановки port indices; reciprocity/passivity; независимые power/KCL checks. Большие relative ошибки малых mutual coefficients не прятать за крупным diagonal; выбирать physical power norm и scalar diagnostics обоснованно.
4. Не менее трёх реальных сохранённых PCB с прямым авторским DSL: источник/порты выбираются по реальным pads/nets, геометрия автоматически извлекается в поддержанной области. Не подменять извлечение вручную введённой RLC-схемой. Не обещать всю плату при расчёте выбранной области.
5. В отчёте: board overlay/выбранная медь/returns/contacts, mesh или basis, токи/потери при заявленной поддержке, frequency response, refinement/error и все допущения. Отдельный audit saved results, negative audit tests и хотя бы один намеренно failed check. Static numerical PNG QA с честной отметкой непроверенных HTML controls при действующем browser запрете.
6. Ранний admission памяти/числа неизвестных/fill/frequency/output; timeout/cancel/повтор. Плотные операторы не обещать масштабировать на всю плату без измерения. До повышения caps — фактический plan/RSS и решение архитектора. Тяжёлые сборки и corpus последовательно по общему слоту.
7. Focused Rust/Node, native/typecheck/build/package, релевантные DC/geometry/P7a/P7b регрессии и общий interchange-тест с P6. Provenance source/addon/input/results, измеренные ресурсы и visual QA записать в pcb-electrical-extraction-verification.md.
8. После приёмки обновить матрицу остатка P7: локальные переходы, channel extraction, PDN, reduction. Не объявлять весь P7 закрытым только по положительному одному via/купонному тесту. Дальнейший подэтап обсуждается с архитектором в пределах исходного поручения продолжить P7.

## 6. Первая контрольная точка

Исполнитель добавляет design addendum: точные supported geometries/band/physical inputs, математический метод, минимальная P6-модель и mapping, источники/эталоны/допуски, ожидаемая стоимость, конкретные три PCB и порядок подэтапов. Передаёт архитектору до крупного production-кода. Можно независимо исследовать имеющуюся геометрию и выполнить малые проверочные эксперименты. Затем реализует законченный блок с тестами и исправляет ревью. Самостоятельные коммиты и переход к P8/P11 запрещены.

## 7. Design addendum P7c — исходный checkpoint, dense feasibility отказ

2026-10-23; исполнитель `codex/analyze-p7-extraction`, база `c8b6617`.
Это предложение законченного следующего блока, не заявление о готовом solver.
Production extraction не изменялся. Прочитаны принятые P7a/P7b semantics и
решения ревью: их quasi-TEM и high-frequency surface-loss пути остаются отдельными.
**Обновление:** родитель разрешил малые probes; исходная dense basis не укладывается
в proposed caps на трёх ROI. §8 сохраняет измерения/отказы и revised probe approach.
Production go пока нет; §7 не является одобренным обещанием dense реализации.

### 7.1. Поддержанный физический класс

Предлагается **локальная thin-volume quasistatic RLC-модель** двух-/четырёхслойной
немагнитной PCB: конечные медные полигоны с отверстиями и CAD fills, явные
положительный и возвратный пути, обычные круглые plated barrels и их конечные
контакты. Это локальный multiport PDN/transition block, а не весь канал/вся PDN.

- Реальные XY-полигоны берутся из принятой geometry preparation, без refill,
  удаления узких шеек/соседних nets или соединения по имени сети. Выбор — существующий
  `Channel.scope` с явным polygon region. Внутри region участвует вся физическая
  медь всех выбранных слоёв, включая плавающие соседние проводники. Clipped source
  geometry, все пересечения границы scope и исходные IDs сохраняются.
- Scope — физическое усечение локальной модели: срезы меди имеют нулевой нормальный
  ток, кроме явно заданных физических портов. Это допущение не превращается в
  состояние исходной платы. Отдельно увеличивается **copper scope**, и отдельно
  artificial dielectric/air domain; изменения не смешиваются с mesh refinement.
  Неподтверждённая локальность не позволяет назвать результат converged PCB model.
- Токи в foil предполагаются постоянными по толщине, но пространственно меняются
  в XY; barrel имеет axial current basis по угловым секторам/сегментам. Взаимные
  магнитные связи вычисляются по конечным объёмам этих basis, включая внутреннюю
  магнитную энергию. Нет high-frequency surface impedance, skin fitting или
  добавленного эмпирического ESL. Junction не считается произвольной 3D формой.
- Явные ordered stackup, все foil thicknesses, dielectric epsilon и
  `lossTangent:0`, conductivity/state/temperature меди, plating/material и
  finished/before-plating convention обязательны. Air — отдельный явно названный
  материал с epsilon=1. Все материалы линейные, isotropic, fixed-temperature и
  nonmagnetic. Нет hidden FR4/copper/plating. Mask, dielectric loss/dispersion,
  magnetic/rough/stacked/micro/slot barrels пока rejected.
- Первичная полоса **DC…100 kHz**; меньшая применимая верхняя граница определяется
  условиями `delta(f) >= 3 max(foil thickness, plating thickness)` для каждого
  металла, `k0 D sqrt(max epsilon_r) <= 0.1`, `foil thickness <= min local lateral
  conductor feature / 4`, и `clear barrel length >= 10 plating thickness`.
  Здесь D — размер всей retained copper области, не только расстояние портов.
  Это консервативные screening guards тонкого объёмного basis, не теорема точности.
  In-plane proximity/redistribution решается в basis и требует mesh convergence.
  При failed guard нет экстраполяции, частотной обрезки или использования P7b.
- Reliable return path между отрицательными electrodes обоих портов обязателен.
  Missing return/contact/fill/outline/material и uncertain topology блокируют
  зависимое извлечение; unknown fill freshness допустима только явно как snapshot
  и даёт conditional. Нет неявной идеальной плоскости GND.
- G3 не объявляется закрытым: только обычный uniform round padstack, выражаемый
  V2, с **явным авторским подтверждением** uniform V2 annuli. Actual antipads —
  отверстия в retained CAD copper, не придуманный clearance radius. Special
  layer padstack/stub/backdrill/microvia без достаточных данных rejected. Схема
  с явно неподтверждённым padstack остаётся в negative evidence.

### 7.2. Математический метод и DC-предел

Метод — Galerkin volume-current integral magnetic operator плюс отдельный 3D
electrostatic FEM Schur complement. Формулы PEEC определяют физические коэффициенты;
существующая DC matrix не получает произвольных L/C.

На согласованной XY triangulation каждого foil потенциал имеет P1 basis `N_n`.
Для каждой extruded triangle используются две постоянные in-plane current basis
`w_i` [1/m²]; для annular barrel sectors — конечные axial volume basis с тем же
сохраняющим ток contact mapping. Нормировка задаёт coefficient `i_i` в A и
не меняет физическое поле `J(r)=sum_i w_i(r) i_i`. Все объёмы/контакты учитываются
один раз на соответствующую компоненту J; overlap foil/axial basis не создаёт
второй медный объём или параллельную копию сопротивления.

```text
R_ij = integral_conductor w_i dot w_j / sigma dV                         [Ohm]
L_ij = mu0/(4 pi) double_integral w_i(r) dot w_j(r') / |r-r'| dV dV'      [H]
A_ni = -integral_conductor grad(N_n) dot w_i dV                          [1]
K_ab = integral_dielectric epsilon grad(N_a) dot grad(N_b) dV            [F]
C = T^T (K_bb - K_bd K_dd^(-1) K_db) T                                 [F]
```

`b` — dielectric conductor-boundary trace nodes, `d` — interior dielectric/air
nodes; T переносит foil/barrel potential на реальные finite-thickness surfaces.
K_dd решается, inverse не материализуется. У внутренних металлических potential
nodes строки C могут быть нулевыми. Края диэлектриков/air/holes согласованы с
конечной медью; conductors исключены из dielectric volume. Artificial outer
potential=0 — конечная аппроксимация infinity, не физический shield/return; domain
doubling обязателен. Нельзя заменить layered dielectric средним epsilon.

Полные symmetric R/L/C сохраняют знаки mutual terms. L интегрируется по finite
prism/annular-sector volumes: разделение self/touching/near/far, singularity-aware
integration, independent quadrature refinement; **нет** centroid self-distance,
epsilon в знаменателе или обнуления неудобного mutual. Если singular quadrature
не подтверждается малыми references, зависимый production шаг не начинается.

Уравнения имеют вид `C vdot + A i = B j_ports`,
`L idot + R i - A^T v = 0`, наблюдение `u_ports=B^T v`.
Отсюда точно следует power identity
`u^T j = d(0.5 v^T C v + 0.5 i^T L i)/dt + i^T R i`.
Это инвариант, не независимый accuracy reference.
При DC исключение локальных foil current DOFs даёт
`A R^-1 A^T = integral sigma*t grad(N) dot grad(N) dA`, то есть P1 sheet
conductivity; finite barrel terms имеют physical length/annular area.
Таким образом DC-предел проверяется и аналитически, и относительно принятого
conduction/DC в совпадающей постановке (одинаковые electrodes/contact rules).
Старый DC solver/geometry не рефакторится.

Ports — finite-area physical pad electrodes с conserving injection и сопряжённым
average-potential observation. PTH требуется явный contact layer; все barrel layers
не становятся одним идеальным electrode. Recovery сохраняет J, oriented barrel
currents, per-source `integral |J|²/sigma`, dielectric potential/E/charge и energy.
В этом блоке нет model reduction; full basis и internal recovery остаются доступными.

### 7.3. Минимальный стык P6 и существующего DSL

Предварительно согласовано с исполнителем P6 (`/root/thermal_review`):
`E=diag(C,L)`, `G=[[0,A],[-A^T,R]]`; P6 владеет PSD/regular-pencil validation и
DC/complex solve. P7 владеет extraction, units/source maps, applicability и recovery.
A — произвольная sparse real dimensionless матрица, **не только ±1 pairwise**.
Никакого разложения mutual в независимые positive scalar RLC.

**Infinity reference не заземляет металлический node.** C к infinity обычно SPD,
а не `C*1=0`. Все metal potentials остаются в AC; A/B имеют нулевые column sums,
и суммирование KCL сохраняет `Q=1^T C v`. Для explicit neutral initial state Q=0.
При DC C выпадает из уравнения: нужны gauge/charge constraints на каждом nullspace
component `A^T`, с независимой проверкой исходной KCL; нельзя удалить произвольный
metal node, назвав это reference. P6 подтвердил необходимость отдельного
`referenceTopology:'infinity'` режима. При source/reference, реально подключённом
к внешнему conductor, boundary задаётся отдельно и меняет физическую модель.
Для нескольких floating components сохраняется `N^T C v`, но его динамика
`d(N^T C v)/dt=N^T B j_ports` может быть ненулевой: ports переносят charge между
components. Neutral initial state разрешается только явно; нет вечной neutrality
каждого component. В DC сначала проверяется `N^T B j_ports=0`. Charge constraints
формируются после сборки всех внешних models/connections, не по resource отдельно.

Concrete **physical C/reference coupon**, независимый от PCB mesher: три тонкие
идеально проводящие концентрические сферические оболочки с радиусами
a=1,b=2,c=4 mm. Все зазоры и внешнее неограниченное пространство — vacuum,
epsilon0=8.8541878128e-12 F/m; phi(infinity)=0. Это аналитический P6 exchange
reference, не обещание извлекать сферические PCB или его использование как via model.
По решению Laplace в сферической симметрии
`c1=4*pi*epsilon0*a*b/(b-a)`, `c2=4*pi*epsilon0*b*c/(c-b)`,
`c3=4*pi*epsilon0*c`; C=[[c1,-c1,0],[-c1,c1+c2,-c2],[0,-c2,c2+c3]].
c1=2.225300110895741e-13 F, c2=c3=4.450600221791482e-13 F.
Два differential ports: inner−outer и middle−outer;
B=[[1,0],[0,1],[-1,-1]], A/R/L пусты. На 100 kHz ожидается
Z11=−j 10728099.705245916 Ohm,
Z12=Z21=Z22=−j 3576033.235081972 Ohm (значения перепроверяются точной формулой;
tolerance `1e-10 relative +1e-6 Ohm`). При neutral Q=0 выводится v_outer=0;
это следствие charge conservation, не наложенный идеальный return.
Если вместо этого фиктивно заземлить **middle** и выбросить его KCL, получится
Z12=Z22=−j/[omega*(c2+c3)], то есть ошибка **50%** в этих entries.
Такой negative interchange обязан провалиться. При DC nonzero constant current
в эту чисто capacitive структуру нет steady solution; initial voltage/charge
должны быть заданы или explicit neutral zero; отсутствие DC не заполняется нулём.

Common-mode elimination допускается только явной power-congruent projection с
charge constraint: для полного SPD C и differential d вводится v=P*d+v_Q,
`1^T C P=0`, `1^T C v_Q=Q0`; C_diff=P^T C P. Оно сохраняет neutral differential
port response/energy, но без сохранённого Q0 и recovery теряет absolute potential,
exterior common-mode field/energy и возможность net-charge excitation. В sphere
coupon Q0!=0 даёт v_outer=Q0/c3 и дополнительную энергию Q0²/(2*c3), которой нет
в neutral two-port. Поэтому P7c отдаёт full C и recovery, а не молча ground-reduced C.

Предложенный additive Resource payload `data.format:'coupled-rlc'` содержит named
internal potential/current DOFs, sparse A/B, full symmetric R/L/C, ordered
physical port identities, explicit reference/gauge treatment, SI units, band,
assumptions/provenance. P6 согласует финальное DTO. Resource может переноситься
через существующий `a.model/a.network`; finite-band S/Z/Y остаётся наблюдением,
а не temporal fit. Для extracted artifacts time unavailable до принятой temporal
applicability; standalone exact ideal P6 coupon time проверяется отдельно.

Пользовательский вход использует существующие `a.port`, `a.channel`,
`a.impedance().target(channel).method('layered')`, stackup/materials/frequency/budget.
Нужны три узких additive schema решения до кода:

1. `port({positive,negative,layer:'BOTTOM'})`: физический surface contact обоих
   pad electrodes. Без layer допустимы только unambiguous single-layer pads;
   неоднозначный PTH отклоняется. Старые DC/line consumers не получают новый смысл
   молча: новые surface-specific ports у них rejected до отдельной поддержки.
2. Existing impedance `.mesh()` получает отдельный typed extraction shape с
   `maxPotentialNodes`, `maxCurrentDofs`, `maxDielectricNodes`, `maxTetrahedra`,
   `maxRefinements`, `maxEdgeMm`, `domainScale`; старые cross-section mesh keys
   и их значения не переосмысляются.
3. Existing impedance `.inputPolicy({cadFill:'snapshot',padstack:'uniform-v2-annuli'})`
   разрешает только эти явно условные сведения, без topology/return/material bypass.

Checkpoint должен утвердить эти минимальные изменения; второго analyzer/runtime,
новых factories/DSL, V2 fields или изменения P7a/P7b inputs не требуется.

Общий независимый interchange coupon: A=I₂, R=diag(.02,.03) Ohm,
L=[[8,2],[2,12]] nH, C=[[200,-40],[-40,160]] pF.
`Z(omega)=[j omega C+(R+j omega L)^-1]^-1`; при DC Z=R.
На 100 kHz: Z11=`.020000024002765595+j .005026500996462809` Ohm,
Z12=`-6.315725198242782e-10+j .0012566528951862992` Ohm,
Z22=`.03000004358393629+j .0075397373278135605` Ohm.
Independent complex 2x2 inverse oracle, tolerance `1e-10 relative +1e-12 Ohm`.
Дополнительно basis-rescaling A=diag(2,.5) с согласованным congruence R/L
должен дать тот же physical Z; port permutation/sign проверяется явно.

### 7.4. Первичные источники и независимая приёмка

Методы прочитаны по первичным публикациям; их код не входит в production:

- [Ruehli et al., IBM, nonorthogonal PEEC (2003)](https://research.ibm.com/publications/nonorthogonal-peec-formulation-for-time-and-frequency-domain-em-and-circuit-modeling):
  circuit interpretation volume/surface interactions для непараллельной геометрии.
- [Müsing/Kolar, ETH, finite/cylindrical partial elements](https://www.pes-publications.ee.ethz.ch/uploads/tx_ethpublications/muesing_COMPEL08_01.pdf):
  finite-volume self/mutual coefficients и cylindrical conductors; не разрешение
  заменять real return общей partial inductance одной via.
- [Passarotto/Pitassi/Specogna, CMAME 392 (2022)](https://www.comphys.com/pdf/cmame_22_vinco.pdf):
  volume-current integral formulation/basis и continuity; наш thin-volume/P1
  mixed discretization — явно отдельный поддержанный subset, не копия full solver.
- [Kamon/Tsuk/White, MIT FastHenry](https://www.rle.mit.edu/cpg/publications/pub55.pdf)
  и [Nabors/White, MIT FastCap](https://www.rle.mit.edu/cpg/publications/pub19.pdf):
  независимые volume-filament R/L и surface-charge C references. FastCap multiple
  dielectric formulation — [Nabors/White, 1992](https://www.rle.mit.edu/cpg/publications/pub73.pdf).
- [Rosa, NBS Bulletin 4, 301–344 (1908)](https://nistdigitalarchives.contentdm.oclc.org/digital/api/collection/p16009coll6/id/14548/download)
  (public domain): finite straight/self/mutual/closed-return benchmarks с их
  настоящими ограничениями, не universal via formula.
- [Münger/Cools, singular volume quadrature (2022)](https://biblio.ugent.be/publication/8767506)
  и [Duffy (1982)](https://epubs.siam.org/doi/10.1137/0719090): допустимые
  singular transforms; volume self-case должен иметь собственное подтверждение.

До production коэффициентов фиксируются exact geometry/materials/BC, source
versions/licenses, raw external input/output/hashes и external refinement.
FastHenry/FastCap — **offline test references**, не runtime/CI dependency; snapshots
не генерируются production helper. Если эталонный executable/source нельзя
воспроизводимо получить или geometry не совпадает, это blocker соответствующего
acceptance-case, а не повод заменить его PSD/convergence или переписать допуск.

Предзаданные acceptance cases/допуски:

| Reference | Геометрия/ответ | Критерий до solver |
| --- | --- | --- |
| Однородная полоса | l=10 mm,w=.5 mm,t=.035 mm,sigma=58 MS/m; R=.009852216748768475 Ohm | R/DC 1%; current/power balance 1e-8 normalized |
| Finite annular barrel | l=1.565 mm, finished diameter=.3 mm, plating=.025 mm,sigma=58 MS/m; R=.0010570927785201747 Ohm | R/DC 1%; сектора/контакты независимо |
| Parallel/layered plate operator coupon | A=10x10 mm²; h=.2 mm,epsilon=4: C=17.7083756256 pF; .1/epsilon2 + .3/epsilon6 mm: C=8.8541878128 pF; exact guarded lateral Neumann BC | C 1%; это assembly reference с этими BC, не finite free-space PCB plate |
| Два тонких finite parallel filaments | l=10 mm,d=1 mm; M=mu0/(2pi)[l asinh(l/d)-sqrt(l²+d²)+d]=4.186470776371762 nH | 1% для separately bounded thin-wire limit; finite-width test сверяется с external geometry, не этим limit |
| Explicit closed return/две связанные полосы | Positive/negative currents, full self/mutual R/L; FastHenry refined raw snapshots; Rosa limits отдельно | 3% каждый nonzero self/strong mutual и loop L; max normalized power error 3%; weak terms имеют signed absolute diagnostics, sign mismatch rejected |
| Via+return-via, finite pads/antipads | Две round annuli с отдельными returns; 2-layer и 4-layer unequal dielectric coupons; exact same finite source geometry в external references | R 2%, L/loop/mutual 5%, C/self/mutual 5%; full complex port Z scaled-coefficient 5% +1 microOhm absolute; relative quadratic-energy gates отдельно |
| Finite plane/PDN и неоднородный multilayer | Plane neck/offset vias/holes, >=2 independently excited ports, nearby floating copper | Те же 5% external R/L/C и port targets; direct DC geometry/FEM comparison 2% при совпадающих BC |
| P6 interchange | Приведённая выше analytic full coupled model, scaled basis и permuted ports | 1e-10 relative +1e-12 Ohm, все complex entries; recovery/power отдельные проверки |

Scaled coefficient normalization для mutual matrix difference:
`Dii=sqrt(max(Mref_ii,Mtest_ii))`, `norm2(D^-1 (Mtest-Mref) D^-1)`;
для zero diagonal требуется zero corresponding row, без скрытого denominator floor.
Эта diagonal scaling **не** является worst relative energy/power norm. Отдельно
на positive range reference matrix проверяется
`norm2(Mref_range^-1/2 * (Mtest-Mref)_range * Mref_range^-1/2)` <= предзаданного
3%/5% порога. Nullspace выделяется rank-revealing decomposition с опубликованным
threshold (предложение: 1e-10 largest eigenvalue); residual/nullspace-leakage tests
обязаны подтвердить, что discarded range не прячет отличную physical energy.
Если rank или nullspace различаются за этим численным guard, это refusal/reference
failure, не automatic projection. Для poorly conditioned almost-null physical
loop дополнительно показываются exact loop currents/energy и absolute discrepancy;
невозможность relative подтверждения не становится pass. R/L и C energy gates
проверяются отдельно. Для dissipative port Re(Z) применяется только positive
range power norm; reactive complex response имеет отдельные coefficient и
physical loop/mode response gates, без ложного названия «power error».
Дополнительно каждый scalar `Mij` сохраняет absolute/relative error и знак;
strong означает `abs(Mref_ij)>=.01 sqrt(Mref_ii Mref_jj)`.
Для остальных absolute acceptance <=`.0005 sqrt(Mref_ii Mref_jj)`;
если это не достижимо, не писать «все mutual подтверждены». Полный complex Z
сравнивается по каждой independently excited port column с diagonal physical
power scaling и отдельно публикуемыми scalar errors. Reference uncertainty должна
быть <=1% по independent refinement; неопределённость не добавляется к 5% молча.

Отдельные mesh refinement и domain doubling: R/L/C/полный port response <=2%
по указанной норме, per-source Joule power/current recovery <=2%; quadrature
refinement magnetic matrices <=.2%. Reciprocity/backward error/power/KCL guards
1e-8 normalized; нет symmetrize/PSD projection/поправочного множителя. Проверить
translation/XY rotation, port/basis permutation, balanced current, disconnected
floating copper, tiny/negative/unknown dimensions и удалённый/неполный return.

### 7.5. Три реальные платы и исходные ports

Все три board JSON сейчас без stackup: любые толщины/epsilon/sigma/plating —
явные **test hypotheses**, не восстановленные CAD свойства. Применяется та же
неизменённая captured copper; исходные файлы/правила не редактируются. Размер scope
до запуска фиксируется по геометрии и сохраняется; невозможность полезной области
при заданных caps/coverage передаётся ревьюеру, не скрывается другим примером.

1. **BandAmpV5**, local VCC_5V/GND: C3.1/C3.2
   (`2f63cacd97ee4389e15/e14`, x=-.2692,y=2.3012/3.7008) → U1.1/U1.2
   (`dbc6321021dda95ae7/e8`, x=.9906,y=.475/-.1753). Candidate copper scope
   x=[-2,4.5],y=[-2.5,5], затем увеличение на 1 mm. Полигонный feed + реальный
   ground pour/return vias; не RF channel. Floating RF copper внутри scope не удаляется.
2. **TPS63060DSCR**, local VIN/GND: C1.1/C1.2
   (`6dcffa1574e9216de16/e15`, x=-3.0505,y=-.8001/-2.7991) → U1.3/U1.4
   (`387993583cdeb7cce31/e30`, x=-1.5011,y=-1.0008/-.5004), TOP.
   Candidate x=[-6,1],y=[-5,1.5], затем +1 mm. Дополнительный same-board
   transition case J1.1/J1.2, BOTTOM contact → C1 TOP, с реальными двумя PTH
   barrels/returns и расширенным source-bound scope. Converter, decaps и L1 не
   становятся идеальными electrical components; это interconnect model без их BOM.
3. **STM32G0_CAN_RS485**, local +3V3/GND: C13.1/C13.2
   (`69f212a373cfdc8de15/e14`, x=-5.3213,y=10.1295/11.5291) → C15.1/C15.2
   (`a0c36f3a3f24c8f3e15/e14`, x=-3.5509,y=11.5291/10.1295).
   Candidate x=[-7.5,-1.5],y=[8.5,13.5], затем +1 mm. Не объявлять такую связь
   идеальной differential line; это два pad-pair observations общей PDN/return.

Для каждого — прямой `extraction.js`, thin launcher, versioned expectations,
complete frequency axes, deliberate failed check и продолжение. Отчёт содержит
board/source/ROI/cut-boundary/return/contact overlay, current basis и dielectric
mesh, J/баррели/потери/E/charge, R/L/C mutual, full Z/S response, три независимые
истории (mesh, air domain, copper scope), applicability и resource plan. Static
PNG из numerical JSON проверяются через view_image; HTML controls остаются
unverified при browser policy. Saved-result audit сверяет физику/ports/units/
arrays/полную ось и отдельно отвергает corrupt IDs, missing return/contact,
changed geometry/mutual sign, lost samples/diagnostics и false-complete/false-pass.

### 7.6. Стоимость, порядок и остаток P7

Новые операторы dense и локальные. Предложенный admission до materialization:
не более 1536 current DOFs, 1024 potential DOFs, **2048 их суммарно**, 50000
dielectric nodes/250000 tetrahedra, 4 physical ports, DC плюс <=17 positive
frequencies, <=3 mesh refinement comparisons и <=3 domain/scope comparisons.
Initial target для реальных patches — <=768 total DOFs; caps не являются обещанием
закончить любой patch. Не повышать их после трудного примера без measured plan/RSS
и решения архитектора. Одна retained extraction за scenario, без persistent cache.

Оценка при total n=2048: real-block complex solve dimension 4096, один dense
numeric array ~128 MiB, factor/work copies могут занять несколько таких массивов;
L <=18 MiB, C <=8 MiB, плюс sparse electrostatic symbolic/numeric workspace,
quadrature и previous retained refinement. Допуск — conservative explicit sum
с реальными symbolic sizes; default plan 1024 MiB, hard refusal при превышении.
**1024 MiB/500 s — explicit probe budgets**, не новые global runtime defaults.
До настройки caps требуются фактические counts/RSS/малые measurements; heavy
experiments используют только слот родителя. Это не OS RSS guarantee.
Operator assembly O(n_current²*quadrature), solve
O(F*n³); board-wide scaling не заявляется. Runtime/time/RSS измеряются на малых
probes до полной series; 500 s per native job, existing disposable cancellation,
ограниченный JSON <=128 MiB. Никаких параллельных больших плат/сборок.

Порядок после этого checkpoint:

1. Утвердить method/subset, три schema gaps и descriptor boundary с P6;
   freeze independent analytical/external references и выполнить малые quadrature/
   DC/descriptor probes. Geometry feasibility и caps показываются до тяжёлого corpus.
2. Native thin-volume R/L, explicit barrels/contacts/returns, 3D layered dielectric
   C, convergence/admission/recovery; focused analytical/negative tests.
3. Общий P6 interchange, минимальная существующая impedance adapter, три direct
   board scenarios + отдельный source-bound transition, независимый saved audit.
4. По слоту родителя native/typecheck/build/package + релевантные geometry/DC/
   P7a/P7b/P6 regressions; full artifacts/static QA/resources/hashes; ревью/исправления.

После приёмки P7c остаются: расширение за thin-volume/uniform round padstack и
skin transition, automatic full channel decomposition/cascading неоднородностей,
широкополосная/большая PDN и доказанная passive reduction с recovery. P7 целиком,
P8 и P11 не закрываются этим блоком; следующий подэтап обсуждается с архитектором.

## 8. Feasibility gate после первого review

### 8.1. Specialized layered extrusion и trace

Предложенный electrostatic mesher — специализированная extrusion, не general
CAD 3D mesher. Common XY planar arrangement содержит точные границы union physical
copper каждого layer, bore/annulus contours, board outline/cutouts, ROI и outer box.
Пересечения constraints сначала разбиваются; holes остаются faces с отдельными
material labels. Только затем constrained triangulation; никакой bbox-замены
полигонов, удаления коротких constraints или snapping ради уменьшения counts.

Общие XY triangles экструдируются по **всем** Z interfaces: foil bottom/top,
dielectric slab boundaries, barrel endpoints и верхняя/нижняя artificial air
границы. В каждом interval physical copper union, bore air, board dielectric и
outer air классифицируются раздельно. Через обычные through bores воздух проходит
по фактическому span; plating — отдельная annular copper область. Each wedge
разбивается на 3 tetrahedra с одной глобальной vertex-order diagonal convention;
shared vertical faces имеют одинаковую triangulation с обеих сторон. Проверки:
positive volumes, <=2 owners каждой face, exact material-interface continuity,
сумма volumes по labels, сохранение всех source contours/holes и min thickness.

Electric mesh исключает copper interiors. Foil potential trace одинаков на верхней/
нижней поверхности foil (объявленная uniform-thickness approximation), P1 в XY;
barrel wall trace линейно по Z между **inner foil faces**, с angular interpolation
к реальным контактам. Узлы на стыках имеют один trace; rows T имеют partition of
unity, сохраняют constant potential/charge и physical electrode work. В этой
тонкой модели vertical potential drop внутри foil/contact spreading 3D не решён:
его нельзя назвать точной полной 3D via-моделью. Barrel/contact reference должен
подтвердить заданные R/L/port tolerances; несовпадение mid-plane length старого DC
и clear-span length здесь показывается отдельно, а не маскируется под regression.

Для initial common XY foil basis T прямой/барицентрический и не пересекает
непредставленные internal P1 edges. Alternative независимых coarse foil meshes
требует imprint всех trace element edges либо отдельной mortar error verification;
его меньшие counts ниже — только нижняя оценка, не уже готовая conforming assembly.

Безопасный memory preflight считает XY vertices/faces, intervals, classified
prisms и верхнюю оценку unique nodes/3*tetrahedra **до** allocation большого объёма.
При symbolic sparse factor plan добавляются actual fill/work sizes. Physical
thin-feature vertices не исчезают из integration mesh при дальнейшей reduction.

### 8.2. Первые реальные counts: первоначальная dense схема не проходит

Geometry-only `debugging/p7-design-checkpoint/extrusion-preflight-original.py`
читает immutable accepted validation polygons, проверяет исходные board hashes.
Это **GEOS/Shapely 2.1.1 CDT lower-bound probe без refinement**, не production
Spade, не новый native binary и не электрический расчёт. Air padding равен
max ROI span; explicit test stack 0/.035/1.565/1.6 mm плюс верхний/нижний air.
Counts фактически построенной planar triangulation/extrusion классификации:

| Исходный ROI | XY nodes / triangles | Dielectric nodes / tetrahedra | Common foil I+V lower bound | Independent coarse foil lower bound | Barrels срезаны |
| --- | ---: | ---: | ---: | ---: | ---: |
| BandAmpV5 | 3189 / 6372 | 18142 / 74763 | 10049 | 4586 | 2 |
| TPS63060DSCR | 1780 / 3550 | 9660 / 35916 | 7882 | 3162 | 2 |
| STM32G0_CAN_RS485 | 1368 / 2730 | 7798 / 31920 | 5161 | 1962 | 0 |

Время ~1.12/.57/.53 s, serial; численные operators не собирались. Последний
вариант independent foil ещё до refinement имеет 1330 current DOFs и 632 potential
nodes, но этим нельзя подтвердить точность/остальные платы. Исходные counts и
negative scope IDs сохранены: у BandAmp срезаны `9e320f36dd4e1431`,
`42c14f692b475a8d`; у TPS `613af030fb5e7e4e`, `936f184249634fb1`.
Это настоящие readiness failures, не успешно моделируемые обрезанные via.

Разрешённая родителем whole-barrel closure: если plating outer circle пересекает
ROI, ROI rectangle детерминированно расширяется до **полного** outer bounds;
повторять до fixed point. Original/final bounds и every added barrel ID фиксируются.
Ни один barrel не удаляется или обрезается для прохождения. Отдельные
`*-whole-barrel-preflight.json` сохраняют исправленный scope, initial evidence не
перезаписывается. Все 3 closure cases физически не режут barrels, но dense caps
по-прежнему **не проходят**: I+V=10283/8340/5161; dielectric nodes/tets
18348/75348, 9888/36192, 7798/31920 соответственно. Это не production admission pass.

### 8.3. Revised candidate: congruent local modes с bounded operator apply

Родитель разрешил малый reduction prototype, **не** увеличение dense cap.
Нужен следующий проверяемый алгоритм; он пока не реализован/не принят:

1. Полная physical integration geometry и sparse A/R/B остаются. Sparse
   electrostatic K_dd факторизуется один раз. `C*x` получается boundary trace →
   sparse interior solve → boundary reaction → T^T; **full C не materialize**.
2. Сначала проверить bounded **stored symmetric L**, повторно используемый на
   всех частотах, без full-space dense factorization. Для constant Cartesian foil
   bases w_x/w_y scalar volume integral один и тот же: хранится один packed K_ff,
   а x-y coupling точно нулевой по dot product. Для axial z barrel bases хранится
   K_zz; x-z/y-z также точно нули. Межслойные x-x/y-y и все z-z interactions
   сохраняются, physical transition/loop coupling остаётся через A. Это доказанная
   структура конкретных bases, не spatial truncation и не diagonal approximation.
3. Self/touching/near interactions требуют singularity-aware volume quadrature;
   medium/far — symmetric adaptive moment/quadrature evaluation с сохранением
   каждого scalar pair. Один mirrored pair хранится один раз. До operator code
   измерить actual unique volume cells, upper-triangle near/far pair counts,
   kernel evaluations и packed storage, включая bases/recovery/sparse C workspace.
   Tiny 32/128/512 prototype сравнит direct kernel с выбранным bounded evaluation,
   <=.2% relative quadratic magnetic energy на positive range и scalar mutual
   gates. Symmetry/PSD проверяются, не чинятся. H2/Chebyshev/new preconditioner
   **не обязательны**: это отдельный fallback checkpoint только если измеренная
   стоимость stored L не помещается в explicit probe budget. Без подтверждённой
   singular quadrature и resource bound нет production implementation.
4. Block-diagonal projection `V=diag(Vp,Vi)`: Ar=Vp^T A Vi,
   Rr=Vi^T R Vi, Lr=Vi^T L Vi, Cr=Vp^T C Vp, Br=Vp^T B. Малые coefficients
   собираются через operator applications, не full matrices; full field recovery
   v=Vp*v_r, i=Vi*i_r и original geometry остаётся доступным. P6 получает обычный
   full **reduced** coupled-rlc descriptor плюс P7 recovery, без собственного solver.
5. Vp содержит все physical DC nullspace/charge modes N **точно**, electrode
   work directions и static port fields. Vi включает static driven currents и
   локальные circulating seeds: sheet stream-function curls, holes/mesh-cycle
   seeds и barrel-loop seeds. Divergence-free projection seed s выполняется
   sparse DC solve `s0=s-R^-1 A^T (A R^-1 A^T)^+ A s`; original A*s0 проверяется.
   Это не заявление, что одних DC modes достаточно для AC.
6. Frequency-dependent enrichment: full-space matrix-free preconditioned solves
   на нескольких заранее выбранных shifts (DC/внутренняя/верхняя полоса), real и
   imaginary snapshots **обоих** potential/current blocks. Krylov/residual
   corrections добавляются blockwise. Полный residual на независимой более густой
   frequency axis выбирает worst shift для следующего enrichment. Circulating/eddy
   currents могут появиться в этих dynamic corrections; tiny norm/low frequency
   не разрешает выкинуть loop без physical error check. Floating charges меняются
   согласно N^T B, все initial constraints остаются explicit.
7. Full-space solves на малых coupons сравниваются с independent reference и с
   reduced response **и J/E/each-source Joule power**, <=2% recovery targets.
   Требуется true R/L/C quadratic-energy norm на positive range, nullspace leakage,
   signed weak mutual diagnostics и separate port scalar errors. Low residual и
   passivity сами по себе не доказывают ни reference accuracy, ни recovery.
   Две frozen frequency axes (training/validation) не совпадают. При недостатке
   modes/caps/convergence — explicit refusal, без relaxing tolerance.

Первичные источники reduction: [Odabasioglu/Celik/Pileggi PRIMA,
TCAD 1998](https://users.ece.cmu.edu/~pileggi/prima-passive-reduced-order-interconnect-macromodeling-algorithm/)
и [Freund SPRIM, ICCAD 2004](https://algos.inesc.pt/projects/mor4less/Freund_04.pdf).
Используется block-congruent passive/reciprocal representation; snapshot/loop
enrichment выше должен проверяться отдельно, а не называться без доказательства
полным SPRIM moment-matching implementation. Если измерения позже обоснуют
magnetic clustering, primary FastHenry multipole work — источник для отдельного
checkpoint, не готовое обоснование нового hierarchical implementation.

Стоимость должна быть измерена до принятия: packed scalar storage
8*[N_f(N_f+1)/2 + N_z(N_z+1)/2] bytes, all-pairs setup остаётся O(N_f²+N_z²)
с различной near/far quadrature стоимостью. Foil geometry cells считаются отдельно
на разных layers даже при одинаковых XY triangles. Near/far classification должна
дать реальные counts до allocation, не обещание subquadratic asymptotics.
Reduced dense factor O(F*r³), retained bases O(N*r); full-space iterative cost
O(iterations*operator_apply) остаётся и требует bounded iterations/preconditioner.
На малых probes сравнить N=32/128/512 integration cells, ranks, pair counts,
kernel evaluations, time и RSS. Proposed reduced r<=256 сначала только probe;
fine-geometry count cap и stored-L memory cap **ещё не согласованы**, прежний dense cap
не переименовывается в fine admission молча. 1 GiB/500 s остаются explicit probe
budgets, не global defaults. Никаких «scales to entire PCB» по asymptotic notation.

### 8.4. Reference acquisition status

Первые raw external strip/return snapshots получены; полный reference gate
**не закрыт** (via, electrostatic C и coupled/recovery comparison ещё впереди).
Скачаны/хешированы official FastFieldSolvers GitHub archives (ignored):
FastHenry2 WinMSVS `10959091782153754071c4e98645c82c11d5c9ac`, FastCap2 WinMSVS
`04eee66446be0629f0607e68f369acad1072dc4d`. Их MIT 2003 notices разрешают применение
по объявленным terms; FastFieldSolvers source license требует acknowledgement и
ссылки. Исторические WRCad headers с internal-noncommercial restriction не выбраны.
Полные origins/licenses/archive hashes — `reference-sources/download-manifest.json`.
Downloaded source не равен проверенному oracle. Console host меняет только report
callbacks/entry point, solver source changes и build hash будут перечислены.
External tools не являются production/обычной CI dependency, не войдут в commit.

Independent design-only arithmetic `physical-reference-probe.py` (stdlib Python,
без production imports) проверил полный sphere C solve/charge и wrong-ground
50% negative; P1 current elimination совпал с direct sheet FEM до
9.095e-13 S absolute. Это проверка формул/единиц/стыка, не extraction validation.
Числа sphere отдельно подтвердил исполнитель P6 по аналитическому выводу.

До полного board production go остаются: native planar/extrusion/trace evidence, revised
operator/reduction cost+accuracy probes, frozen refined matching external R/L/C
и joint port references, singular self/near integrals и обсуждённые resource caps.

### 8.5. Measured small-probe checkpoint, 2026-10-23

Сохранены исходные negative scopes §8.2. После whole-barrel closure actual
current I / potential V / unique foil volumes / axial sectors:

| ROI | I | V | foil volumes | axial sectors | packed scalar L bytes |
| --- | ---: | ---: | ---: | ---: | ---: |
| BandAmpV5 | 7748 | 2535 | 3682 | 384 | 54,834,584 |
| TPS63060DSCR | 6244 | 2096 | 2978 | 288 | 35,818,776 |
| STM32G0_CAN_RS485 | 3842 | 1319 | 1873 | 96 | 14,077,256 |

Geometry-only bbox-gap classification `near<=2*maxDiameter`, `medium<=6*maxDiameter`,
`far>6*maxDiameter` даёт upper-triangle foil pairs соответственно:
932302/2108535/3739566; 795852/1168043/2471836; 238480/466956/1049565.
Эти пороги **не** quadrature error-admissibility. Операторы ещё не рассчитаны;
quadrature setup остаётся quadratic и может не пройти time cap. Python geometry
count pass занял 2.20/1.47/1.09 s, не kernel benchmark. Retained basis при r=256
занимает дополнительно 8*r*(I+V) bytes; symbolic sparse factor fill и operator
workspace должны быть включены в admission до allocation. При текущем storage
оснований вводить обязательный H2 нет.

Ignored `coupon-extrusion.py` создал полные connectivity/trace JSON и static PNG:
one-layer — 74 XY nodes, 142 XY triangles, 2370 dielectric tetrahedra, 518 active
nodes, 132 trace rows; two-layer via+explicit return — 216/426/5268/1512/936,
из них 128 trace rows имеют два axial endpoint supports. Dielectric+metal volume
balance <=5e-15 relative; face multiplicity <=2, все internal dielectric boundary
faces имеют exact removed-copper neighbor, unclassified cracks=0; trace weights
неотрицательны и sum=1. Это GEOS geometry/trace feasibility, **не** native field
accuracy или production-quality mesh pass. Static PNG просмотрен; HTML/browser
controls не проверялись.

Сохранены spoiled/negative facet/trace audits: повторный `covers(vertex)` на
floating-point boundary пропускал source trace, классификация крошечного triangle
по его representative point меняла ownership. Исправленный probe наследует
material ownership planar face и incident-node tags; координаты не snapped,
slivers не удаляются. Native mesher должен сохранять те же topology identities
и отдельно отказывать на conditioning/quality, не объявлять малые slivers здоровыми.

FastHenry2 WinMSVS source `10959091782153754071c4e98645c82c11d5c9ac`, clang x64:
console build exit0, 7.79 s; binary SHA256
`4aa0b6edcb27606d2b09403f468259d054454c51bc0b8d4e7d38be179c896dcd`.
Build RSS не измерялся. Tiny strip/parallel-return runs nwinc=nhinc=1/3/5:
все exit0, 0.025–0.215 s each, sampled peak process RSS <=9.1 MiB.
10mm x .5mm x .035mm copper: R=0.00985222 Ohm против analytic
0.009852216748768475; uniform self L=8.272304610981235 nH по независимому
convolution/Duffy reference, orders128/256/512 refined до <1e-12 relative change.
FastHenry self output совпадает в пределах его 6-digit precision; mutual для
parallel bars при center spacing1mm ~4.2298 nH. Square .5mm coupon дополнительно
имеет независимые self 0.14198058291191782 nH и mutual 0.025533210945850166 nH,
совпадающие с frozen FastHenry uniform-filament result.
Authored inputs, raw external matrices, hashes и acknowledgement сохранены в
`tests/fixtures/analysis-local-extraction/`; vendor source/binary остаются ignored.

Малый near/self candidate интегрирует source-z и fan-radial coordinates точно:
`F(k)=.5*(asinh(k)+k/(hypot(1,k)+abs(k)))`, где
`F(k)=integral_0^1 r*asinh(k/r)dr`; signed apex fans сохраняют outside target.
Edge и outer triangle/z quadrature остаются численными. На длинном strip две
треугольные halves дают self error 34.7/12.9/6.19/3.61/1.58/0.838% при orders
4/8/12/16/24/32: **последний не проходит .2% gate**. Это сохранённый negative
для quadrature/triangle aspect, не успешный operator. Well-shaped square намного
лучше, но anisotropic/refined mesh и adaptive rule должны быть проверены отдельно.

Родитель разрешил isolated native core primitives/mesher/R-L и C coupon с small
focused tests. Board/DSL integration и полная accuracy claim остаются gated;
full-space vs reduced internal J/E/source power evidence ещё не получено.

### 8.6. Meshing blocker taxonomy и следующий design checkpoint

Геометрический аудит `geometry-taxonomy.py` сохраняет общие и independent-layer
meshes отдельно, exact F64-coordinate triangle area через Decimal и distance до
исходных TOP/BOTTOM boundaries. `maxEdge²/(2area)>1e8` в common XY:
BandAmp 1, TPS 79, STM 49; independent meshes: BandAmp 0/0, TPS 1/1,
STM 13/0 (TOP/BOTTOM). Нельзя все случаи назвать реальным узким copper feature.

1. Numerical/construction slivers: BandAmp worst triangle имеет area~4.86e-19mm²,
   aspect~4.25e14; near-collinear intersection coordinates дают плохо определённую
   area в ordinary F64. Нужны robust construction/shared vertex ownership,
   provenance и отдельный conditioning refusal. Sign-correct orientation одна не
   гарантирует relative area/gradient accuracy.
2. Single-layer bad triangulation/contours: independent TPS 2 и STM TOP 13 cases
   остаются. Это **не доказательство physical neck**: причиной могут быть почти
   дублированные boundary vertices или collinear constrained edges. До объявления
   реального узкого контура требуется source-bound opposite-boundary/clearance
   witness; classification пока `unresolved single-layer contour/triangulation`.
3. Cross-layer projection artifact: STM worst BOTTOM triangle целиком навязан TOP
   boundary, при distance до BOTTOM boundary .39–.88mm; две TOP vertices отличаются
   примерно на2.9e-10mm. Common XY переносит тонкий projected feature в здоровую
   другую physical surface. Endless subdivision не устраняет такое ограничение
   общей 2D triangulation.

До полного mesher implementation предлагается сравнить два решения:

- Controlled coordinate alignment — допустимо только как **явное** дополнительное
  geometry approximation, не silent snap. Нужно заранее объявить displacement
  bound, original/approx geometry и source recovery, сохранить per-layer components,
  holes/contact/net separation, доказать отсутствие новых contacts и отдельно
  mesh/geometry/field-energy convergence. Малое абсолютное смещение не даёт само
  по себе relative energy guarantee около узкого gap/neck. Поэтому этот вариант
  пока не выбран и не реализован.
- Специализированный **layered Galerkin basis с independent XY meshes на z planes**
  избегает наложения новых unknowns на overlay slivers, сохраняя original geometry.
  Это следующий минимальный candidate для review, а не production promise.

Во втором варианте plane k имеет healthy source-conforming P1 mesh и potential
`phi_k(x,y)`. В slab `[z_k,z_{k+1}]`:
`phi(x,y,z)=(1-s)*phi_k(x,y)+s*phi_{k+1}(x,y)`, `s=(z-z_k)/h`.
Каждая plane continuous; один и тот же plane vector переиспользуется соседними
slabs, поэтому space H1 conforming без mortar penalties, multipliers или нового
solver. XY meshes соседних planes не обязаны совпадать. Межслойный overlay нужен
**только для integration**, не для nodal basis/unknowns. На маленьком overlay cell
используются gradients исходных healthy triangles, не inverse overlay area.

For each overlapping triangle pair integration uses exact degree-two polygon
moments and analytic z integrals. With `I0=integral eps dz`,
`Ibb=integral eps*(1-s)² dz`, `Itt=integral eps*s² dz`,
`Ibt=integral eps*s*(1-s) dz`:
`Kbb=integral[Ibb*gradNb.gradNb + I0/h²*Nb*Nb]dA`,
`Ktt=integral[Itt*gradNt.gradNt + I0/h²*Nt*Nt]dA`,
`Kbt=integral[Ibt*gradNb.gradNt - I0/h²*Nb*Nt]dA`.
One symmetric energy assembly creates both off-diagonal blocks. Shared planar
ownership and robust polygon moments still matter; tiny integration fragments
are accumulated, not dropped. R-tree enumerates actual overlap pairs before
allocation; planned terms<=36 per pair plus trace/factor workspace. Cost depends
on overlap count and sparse factor fill, not the rejected common-XY node count.

Foil-thickness slabs use the **same** XY mesh on both faces, preserve exact foil
side walls and exclude actual copper volume. Core slabs use independent TOP/
BOTTOM plane meshes while common bore/barrel/board-outline constraints are retained
on every plane: vertical material domains are unchanged. A dielectric z interface
is integrated piecewise exactly; z refinement remains an independent field-space
convergence check. Barrel-wall ring coordinates/subdivisions are common constraints
on adjacent planes; trace remains linear clear-span and uniform through foil.
All metal potentials, full C/common mode and explicit far boundary stay unchanged.
Current R/L supports can use healthy independent per-layer foil triangles, rather
than introducing current DOFs solely for another layer's projection.

Before adoption: source-bound taxonomy of unresolved single-layer cases; one-layer
and via/return coupon comparison with independent conforming tetra FEM; analytical
layered plate, frozen FastCap finite-plate full C and sphere checks; separate
plane-XY/z/domain convergence, positive-range energy/nullspace guards, original
trace/charge/source recovery; actual overlap counts and budgets for all3 ROIs.
No new general CAD3D mesher or unreviewed full-board implementation is authorized
by this candidate. If robust same-layer construction/conditioning still fails,
that ROI remains a readiness refusal.

### 8.7. Дополнительные reference/cost результаты и оставшиеся отказы

Native core primitive tests на isolated jobs1 target: 9/9 pass (sheet DC
condensation/Joule recovery, layered tetra topology/material/source/budget,
series plate sparse C apply, explicit-far full-C common mode, finite prism
self/touching/return, bounded p4 remainder и physical basis normalization).
Это не board/DSL admission. Следующий exact-source polyhedron increment ещё
ожидает focused native compile; никакой build claim к нему не относится.

Bounded p4 prototype, N=32/128/512 well-shaped thin prisms с close non-touching
pairs: preparation .00011/.00023/.00087s; near/self order8/12+directed symmetry
.254/.988/3.940s; far p4 .00012/.00190/.03066s против smooth product q3
.00479/.07514/1.20770s. Far q4 samples учитываются отдельно; all compute для512
5.294s, raw matrix serialization довела stdout wall до15.173s; complete probe+
saved audit17.73s, sampled native process peak9.1MiB. Prototype делает
дополнительный initial per-pair classification evaluation; это есть в total,
но не в повторном timed p4-only числе. Production cost нельзя оценивать только
этим p4-only числом. Матрицы сохраняют одно значение пары и mirror.

True positive-range relative quadratic-energy norm **замены far coefficients**
<=2.25e-10, rank512, nullspace dimension0; max near order change .1002%, directed
asymmetry3.94e-6. Near coefficients общие в обеих compared matrices: loop change0
**не доказывает near accuracy**, отдельные independent near references обязательны.
Actual coarse BandAmp bbox-near count~932k × измеренные5.13ms/pair даёт~4.8ks:
текущий fan n^4 не проходит explicit500s probe budget. Cap не увеличен.

Альтернатива near source potential: собственный divergence-theorem вывод
`.5*sum_faces h*(sum_edges d*log((ra+rb+l)/(ra+rb-l))-h*omega)` для **полного
volume prism**, с exact edge logarithms/solid angles и численным outer volume.
Это тот же uniform-density `1/r` integral, который рассматривают первичные
[Werner/NASA1994](https://ntrs.nasa.gov/citations/19950035213) и
[Werner/Scheeres2005](https://deepblue.lib.umich.edu/bitstream/handle/2027.42/42570/10569_2004_Article_4621.pdf);
формула/probe не заимствуют vendor code. На длинном box self error при outer
orders4/8/12/16/24/32: .319/.0295/.00548/.00144/.000141/.0000163% к independent
convolution/Duffy reference. Uniform explicit return с physical gap1um совпадает
с frozen FastHenry text в пределах~7e-7 relative на order8. Это не заменяет
actual sliver/contact conditioning gate и ещё не доказывает real-ROI runtime.
Edge-limit/cancellation cases не softened: primitive должен явно отказать, если
finite limiting expression не вычислена надёжно.

FastCap2 source `04eee66446be0629f0607e68f369acad1072dc4d`, compiled CLI binary
SHA256 `537aa632111eddd63c8d25f1c1ef90580cdae8a4eecc9749802229aa78796dd1`.
Raw **unsymmetrized** C exported до стандартной vendor symmetrization; оба вида
сохранены. Finite vacuum plates1mm square x35um, inner-face gap.965mm: panel
refinement192/768/3072/12288 at multipole order4 даёт self47.686/48.130/48.304/
48.365fF, strong mutual -15.431/-15.659/-15.747/-15.776fF. Order6 at12288 panels
изменяет coefficients <.004%,2.55s,180MiB. Sphere8192 panels order4 отличается от
analytic C на.0553%; этот sphere тест проверяет **внешний reference tool**, не
обещает sphere geometry в PCB layered extractor. Sphere8192 order6 превысил
explicit256MiB tiny cap и остановлен; отказ сохранён, бюджет не повышен.
Меньший sphere2048 order6 выполнился. Raw results, authored-input generator,
source/binary/input/output hashes/acknowledgement frozen в
`tests/fixtures/analysis-local-extraction/fastcap/`.

Layered Galerkin coupon prototype сохраняет full C и charge/energy residual~1e-15,
но это не accuracy. Coarse v1 n2,D2.5mm: self58.77fF против FastCap48.364fF;
external positive-range energy error~37%, **FAIL**. v2 добавляет core z grading;
n4,D20mm даёт self52.23fF, energy error~12%,14941nodes,220MiB. v3 смешивает XY
edge и far-z grading; n4,D10mm:23479nodes,475MiB,12.16s; self51.14/50.85fF,
mutual-15.39fF, full quadratic energy error~9.4%, **FAIL**. Mixed v3 changes не
используются как isolated XY/z/domain convergence. Все прежние inputs/outputs
сохранены. Static audit PNG просмотрен; browser/HTML automation не применялась.

Independent-plane real-ROI geometry-only counts (не refinement/field pass):
BandAmp20588 potential plane nodes и24045 adjacent different-mesh overlap pairs;
TPS10296/16336; STM7104/9404. Это seven coarse planes; остальные same-mesh slabs
и full symbolic fill учитываются отдельно. Tiny overlap fragments сохранены.
Exact F64 ring-coordinate **и edge-endpoint** tuple equality между BOTTOM/CORE/TOP
проверена для всех3 coarse cases; соответствующие hashes внутри каждого fixture
совпадают (`layered-overlap-counts.json`). Distance<=64 coordinate ULP применяется
только для membership диагностики, координаты не сдвигаются и equality не tolerance
based. Это prerequisite, не полный native H1/trace proof. TPS/STM unresolved
single-layer construction/conditioning failures всё ещё остаются.

### 8.8. План перед следующим field refinement

Новых больших C solves после v3 не запускать до memory/symbolic checkpoint.
Следующий контроль раздельный:

1. Зафиксировать exact copper/ports/trace, foil planes и **внутренний** graded z.
   XY-only comparison меняет только healthy plane XY resolution; z и far box
   неизменны. Unknown material interfaces не усредняются.
2. Z-only comparison сохраняет XY meshes/geometry и boundary data; добавляет
   graded planes около обеих foil faces и отдельно дальние air planes. Healthy
   plane vectors переиспользуются соседними slabs. Different finite spaces могут
   быть nonnested; нужен independent reference и positive-range energy test, а
   не требование coarse equality с tetra FEM.
3. Domain-only comparison сохраняет **все** исходные internal XY/z coordinates,
   включая старые outer endpoints; расширение использует заранее заданную
   canonical graded sequence (например D4/8/16), не удаляет старую boundary plane
   при появлении новой. Только тогда изменение отражает outer boundary removal
   при фиксированном внутреннем пространстве. Простое D5→D20 с потерей z=5 не
   считается таким control.
4. До numeric factorization подсчитать nodes/free nodes, overlap pairs, sparse
   pattern/nnz, bandwidth после фиксированного graph ordering, mesh/trace/kernel/
   recovery arrays. Pattern-only native symbolic Cholesky сообщает len_val и
   factor/solve scratch, после чего admission повторяется. Полный dense C не нужен.
   Для natural/RCM ordering conservative band-fill storage upper estimate
   `16*Nfree*(bandwidth+1)` bytes (F64+usize) **не** объявляется measured fill для
   другого ordering; actual symbolic обязателен. Include simultaneous COO/CSC,
   symbolic/factor/workspace peaks; early-drop construction temporaries.
5. Python v3 retained boxed-list triplets увеличивали peak; explicit preallocated
   numeric COO и release temporaries — разрешённая implementation optimization,
   но её выигрыш пока не измерен. Она не исправляет C accuracy. Prototype node/
   triplet caps и1GiB/500s остаются явными; больший input требует слота/плана,
   не silent cap increase.

Если tensor-product coupon grids тратят слишком много узлов на far planes,
допустимый следующий **малый** candidate — source-conforming graded planar CDT
с редким far seed cloud и локальным refinement около metal, с bounded nodes/
quality/constraint audit. Это 2D plane work существующего layered space, не новый
CAD3D mesher. До запуска более крупного поля должны быть actual pattern/fill
counts и согласованный resource slot; failed grids не гоняются как будто точность
уже подтверждена.


### 8.9. Exact-source gate и пересчёт independent-layer supports

Focused isolated Rust increment: **11/11 pass**, compile25.08s, tests0.38s,
`rust-local-extraction-polyhedron.log`. Дополнительно к §8.7 проверены длинный
10x.5x.035mm self и explicit-return gap1um против независимых frozen references;
planar embedding теперь отвергает geometric overlaps/раздельные IDs одинаковой
координаты, которые один combinatorial edge-incidence audit не обнаруживал.
Тестовый run предшествовал additive anisotropic-rule API; к этому следующему
increment build claim пока не относится.

Повторный N32/128/512 prototype с exact-source near: prepare .00021/.00049/.00199s;
near XY/z8/12 plus directed symmetry .0629/.2487/.9729s; old fan q12 plus symmetry
.2209/.8731/3.4108s. Теперь initial classification посчитан ровно один раз и
измерен вместе со всеми far evaluations: .00020/.00269/.03508s. Full compute
.289/1.207/5.702s включает old comparator, direct far reference и q4 samples;
buffered serialization измерена в полном process wall. Probe+audit7.99s,
peak10.3MiB. `kernel-polyhedron-cost-audit.json` хранит source/binary/raw hashes.
`farP4S=0` в его legacy field означает отсутствие отдельного repeated timer,
а не бесплатное вычисление: actual cost находится в classificationAllPairsS.

На этих **well-shaped synthetic** supports max polyhedron q8/q12 change5.91e-6,
directed asymmetry2.07e-8; true positive-range energy difference относительно
старого fan q12 <=6.251e-4, rank512/no nullspace. Это comparison двух numerical
near rules, **не независимый full-space oracle**. Loop difference6.25e-4 теперь
включает near replacement. Actual sliver/contact и отдельные XY/z refinements
остаются gate. Никакой полный L/corpus run не выполнен.

Старый near932302 получен на common XY overlay и не используется как актуальный
план. `independent-current-counts.py/json` заново строит per-layer meshes с common
board/bore/barrel constraints и выбирает только Cu supports. Prepared Cu contours
с source-board SHA остаются неизменными; old negative scopes и whole-barrel
closure сохранены. Geometry-only GEOS counts, не native producer admission:

| ROI | foil volumes | axial currents | unique prism volumes | I / V | packed scalar L bytes | foil bbox near | foil non-p4 |
|---|---:|---:|---:|---:|---:|---:|---:|
| BandAmp |2962|384|3730|6308 /2166|35696984|861052|1442757|
| TPS |2178|288|2754|4644 /1586|19316376|555273|921125|
| STM |1002|96|1194|2100 /824|4057272|112864|177802|

Foil bbox near означает gap<=2*max bounding diameter, не quadrature acceptance.
Conservative p4 preclassification использует `q=(rA+rB)/R<1`, absolute remainder
`VaVb/R*q^5/(1-q)` и exact integral lower bound `VaVb/(R+rA+rB)`: resulting
relative truncation bound `q^5*(1+q)/(1-q)<=2e-4`. Это не F64 interval/full energy
admission. P4 nonadmitted pairs больше bbox-near. Все same-direction interlayer
couplings сохранены; единственный foil scalar kernel используется x/x и y/y.
Foil x/y и foil/axial z равны нулю по dot product Cartesian bases. Axial sector
quadrilateral имеет два prism supports: z/z pair требует four subintegrals
(three independent terms on self). Unique axial prism upper pairs295296/166176/
18528; эта высокая z-elongation не покрыта well-shaped foil timing.

Shape proxy maxEdge²/(2area) по пяти bins <=10, <=100, <=1e4, <=1e8, >1e8:
BandAmp2094/798/68/2/0; TPS1515/581/74/6/2; STM755/196/32/6/13.
Полные pair histograms worst-shape x distance, same/different-layer и exact
geometry hashes сохранены. Ни один unresolved triangle не удалён. Geometry
preparation1.08/.67/.35s; линейная проекция только measured prism/moments preparation
по synthetic512: .0145/.0107/.00465s, не actual native preparation promise.
Near integration projection до anisotropic/actual-shape measurements отсутствует.

Круглые bore/barrel constraints — **generated approximation of authored circular
facts**, не authored polygons. Независимые прежние 28/32 chord grids сохранены
как diagnostic alternative; точное сохранение обоих polygons требует union56
angular sectors и повышает counts до4114/3042/1290 foil volumes. Для подтверждённых
round-drill radius/centre выбран единый angular registry32: outer radius chord
deviation<=1um, inner deviation ещё меньше, input radii/centre и source ID
сохранены. Это исходный выбор circle approximation, без near-angle snap и без
замены arbitrary pour/hole polygons. Native producer должен использовать этот
registry и для доказанно generated drill cuts, и для foil/field/barrel traces;
legacy prepared Cu hole polygons в count-only snapshots не объявляются новой
conforming producer geometry. Geometry/energy refinement и exact ring-segmentation
проверяются повторно после native construction. Пока full-board gate закрыт.

### 8.10. Раздельный quadrature и constrained pattern checkpoint

`anisotropic-near-probe.py/json` оставляет exact finite prism source potential,
но независимо выбирает outer XY/z orders. На frozen long/square self-touching и
explicit-return references XY12/z4 имеет relative errors<=5.5e-5. Это не общее
правило admission: actual unresolved foil self z2 отличается на1.4–2.1%, z4 до
.396% от XY48/z16 и **не проходит** .2% gate. Axial self z4 отличается до.513%,
z8 до.0713%; XY12 при z16 отличается от XY48 примерно1e-9. Separate XY24→48 и
z8→16 refinement этого selected set не подтверждает независимую точность actual
sliver geometry или всего оператора. Python probe10.1s/~46MiB — не native cost.
Малый production-source standalone optimized native probe выполнен отдельно
после освобождения общего resource slot; результат ниже. Полного L нет.

Первый `layered-pattern-plan.py` сохранил n4/D4 pattern и остановился на n8/D4
TOP: missing exact source-ring edges `(15,16)` и `(16,17)`. Forensic geometry
`planar-constraint-forensics.json` установила причину: `scipy.spatial.Delaunay`
вообще не enforcing constraints. Два unconstrained edges пересекали source-square
boundary между outer r=.505mm ring и interior seed(.39625,.39625)mm. Поэтому
ownership был неверен **до** assembly. Constraints/seed coordinates не удалялись
и не двигались; failed original probe сохранён неизменным.

Разрешённый synthetic-coupon `layered-pattern-plan-cdt.py` сохраняет все прежние
vertices/seeds и exact source square. Inner convex source square triangulation
и GEOS constrained CDT каждого graded square annulus дают явный domain ownership.
Проверки до pattern: every required ring edge exact, every original vertex used
без изменения F64 coordinates, face incidence1 только снаружи/2 внутри, source
ownership, no-overlap и full coverage. Plane-only n4/n8,D4/8/16 прошли; n8 healthy
worst XY aspect proxy25.25. Это synthetic plane constructor, **не** новый PCB mesher.

| Controlled pattern | nodes / free | sparse pattern nnz | native base plan bytes | natural-band fill upper bytes |
|---|---:|---:|---:|---:|
| XY n4,D4,zbase |3025 /2315|43845|5567152|7519120|
| XY n8,D4,zbase |6825 /5219|99375|12782160|38996368|
| XY n8,D4,zfine |14469 /11481|223953|27037472|85786032|
| XY n8,D8,zfine |17385 /14205|279869|33006592|120685680|
| XY n8,D16,zfine |20557 /17185|341161|39522528|163601200|

Pattern-only full set6.36s, cumulative Python process peak80.4MB. Binary patterns,
per-plane audits, hashes/manifest в ignored `layered-pattern-cdt/`; предыдущие
pattern files не перетёрты. Domain extension сохраняет canonical internal z
planes и все прежние graded XY rings. Pattern представляет conservative clique
upper bound36 local terms/overlap, а band number — upper для указанного natural
ordering. На этом pattern checkpoint native symbolic fill/scratch, numeric factors
и fields ещё не запускались; их последующий отдельный результат ниже. Pattern
не является C accuracy/admission pass; прежние full-C energy FAIL сохраняются.

Перед production plane construction изучен существующий
`native/analyze/src/conduction/mesh.rs`: pinned Spade CDT уже вставляет constraints
с splitting и bounded refinement, сохраняет boundary source paths и проверяет
subedge coverage/incidence/connectivity. Его terminal-only alignment и удаление
diagnosed collinear boundary faces не являются разрешением переносить эти операции
на original P7 copper/field geometry. Следующий узкий adapter должен переиспользовать
CDT machinery с exact shared ring registry/source ownership и отдельным refusal
на unresolved conditioning; unconstrained Python prototype в production не переносится.

### 8.11. Native anisotropic cost и раздельные constrained C fields

`anisotropic-native-cost.py` импортирует frozen production `prism.rs` и
`polyhedron.rs` через standalone rustc1.98.1 `--edition=2021 -O -C codegen-units=1`.
Compile2.10s, runtime4.64s, sampled native peak4.66MB при explicit60s/128MiB.
18 selected cases: четыре frozen uniform-box coupon sums и actual foil self,
edge-neighbour/axial cases трёх ROI. Source/case/raw/binary hashes и compiler flags
в `anisotropic-native-cost-audit.json`. Every timed rule включает construction
и **обе directions**; separate XY24→48 при z32 и z16→32 при XY48 служат refinement
diagnostics, не independent reference для real slivers. Finite actual pairs на
XY12/z4 занимают .50–.78ms, z8 .92–1.52ms; обязательные refinement checks добавляют
стоимость. Даже этот selected-pair timing не обосновывает full-ROI500s budget.

Frozen coupons XY12/z4 within5.5e-5 independent relative error. На actual axial
z4 self error~.516%, unresolved STM foil~.401% — **FAIL** .2%; z8 selected finite
cases <=.0735% к XY48/z32. Actual STM slender edge neighbour supports668/556
отказал **на всех восьми orders**: `LOCAL_POLYHEDRON_RANGE`, nonpositive/nonfinite
point potential, без clipping/softening. Это сохранённый cancellation/geometry
blocker; не все actual pairs пригодны даже при дорогом quadrature. Ни full L,
ни full magnetic energy/real-board operator admission не выполнялись.

Для C сначала `layered-native-plan.rs` читает saved conservative CSC pattern и
вызывает тот же pinned faer0.24.4 AMD symbolic Cholesky через isolated target rlib,
без target copy/crate rebuild. Standalone wrapper/source/compiler/rlib/binary
hashes сохранены. Native numeric LLT/solve используют те же planned values/scratch;
точные numeric CSC indices/pointers сверяются с pattern до factorization.
Python assembly использует preallocated typed COO, exact degree2 polygon moments,
analytic z и прежний full metal trace, без retained boxed scalar triplets.

| Fixed control | native factor values / bytes | numeric scratch bytes | C11 / C12 fF | full-C relative energy error | whole solve wall / combined peak |
|---|---:|---:|---:|---:|---:|
| n4,D4,zbase |547858 /4382864|744639|55.214 /−14.307|25.562% FAIL|1.65s /75.9MB|
| n8,D4,zbase |1375389 /11003112|1682815|54.428 /−14.167|23.573% FAIL|2.29s /89.9MB|
| n8,D4,zfine |4299228 /34393824|3782527|53.631 /−14.043|21.509% FAIL|3.12s /128.4MB|
| n8,D8,zfine |6925949 /55407592|4725311|52.289 /−15.265|13.636% FAIL|3.88s /155.8MB|
| n8,D16,zfine |7650940 /61207520|5750975|51.688 /−15.848|10.003% FAIL|4.05s /168.5MB|

Все five solves прошли explicit512MiB/60s resource cap. Combined peak — sampled
sum Python/native working sets, не один factor allocation. Factor times первых
трёх .031/.082/.268s; native symbolic/scratch отдельно от предыдущих band upper.
Full C positive range rank2, без hidden conductor ground; reciprocity/charge/work
residuals <=5e-14. Это numerical consistency, **не accuracy**. Isolated XY energy
change1.61%, z1.70%, domain D4→D8 6.93%, D8→D16 3.30%. Domain checks сохраняют
все прежние XY triangle coordinate tuples и canonical internal z planes exact.
Saved matrices/manifests и первоначальные failed diagnostics не перезаписаны.

Независимый `audit-constrained-formulation.py` проверил exact raw FastCap input
SHA256, bounds обеих 1mm square x35um plates и inner-face gap965um в SI. Full
metal/sidewall potentials и explicit far-zero box соответствуют постановке;
units/hidden-ground/half-energy mismatch не найден. Другой Green-theorem polygon
moment вывод и direct3D gradient quadrature с piecewise epsilon совпали4.51e-16,
affine field energy2.22e-16, constant null mode pass. Common energy errors для
D4/D8/D16:21.509/13.636/10.003%; differential:5.525/5.336/5.308%. Значит outer
boundary объясняет существенную common-mode часть, но domain extension alone
не закрывает spatial FE floor и full-C5% gate остаётся **FAIL**. Static PNG
`constrained-field-audit.png` просмотрен; browser/HTML controls не проверялись.

Следующий согласованный checkpoint — **план**, до новых solves: на одном D16
отдельно локальный XY refinement у metal edges/foil sidewalls и отдельно z
refinement около обеих foil surfaces. Сохраняются existing constraints, trace,
outer box и прежние координаты; actual pattern/native symbolic budgets показать
до запуска. Baseline — saved n8,D16,zfine, не повторный solve. Никакого blind
uniform refinement, увеличения caps или нового general solver этим не разрешено.


### 8.12. Paired D16 XY/z checkpoint, 2026-10-24

После освобождения Thermal heavy slot выполнены только два заранее согласованных
CPU/F64 solves, каждый под unchanged supervisor60s/512MiB. Baseline
`n8-d16-zfine.json` переиспользован без solve. Patterns и actual pinned-faer native
symbolic были подготовлены до запуска: XY24461nodes/20641free, factor75.93MB;
z24601nodes/20521free, factor76.24MB. Exact numeric CSC совпал с planned pattern.

XY-only делит source-square edges .125→.0625mm и r=.505 ring edges
.12625→.063125mm; все61zplanes fixed, old far XY triangles outside .52mm сохранены.
z-only сохраняет337XYnodes/plane и все61oldplanes, добавляя12planes возле foil
faces/sidewalls. Source constraints, exact geometry и полный metal trace сохранены.

| D16 control | C11 / C12 fF | full / common / differential external energy error | baseline-relative full / common / differential energy change | whole wall / combined peak |
|---|---:|---:|---:|---:|
| saved baseline |51.688 /−15.848|10.003 /10.003 /5.308% FAIL|—|reuse|
| XY metal edges |51.646 /−15.826|9.9235 /9.9235 /5.1978% FAIL|.11006 /.07272 /.10505%|4.94s /192.28MB|
| z foil surfaces |51.685 /−15.847|9.9996 /9.9996 /5.3023% FAIL|.005702 /.003479 /.005700%|4.47s /190.26MB|

Full error — positive-range quadratic norm против frozen FastCap plates32/o6;
common `[1,1]` и differential `[1,-1]` проверены независимо against той же
symmetric quadratic reference. Scalar diagnostics сохраняют raw unsymmetrized
entries. Changes нормированы на новое поле; небольшое изменение не означает
accuracy. Reciprocity/charge/work checks pass; max free residual1.16e-15,
max energy residual1.18e-13, max charge residual6.43e-13. **Оба full-C5% gate
остаются FAIL**, включая differential. Эти локальные controls не закрыли spatial
error; сами по себе они не локализуют remaining error и не доказывают mesh/domain
convergence. Никакие tolerances/caps/physical terms не изменены.

Evidence: `layered-constrained-field/paired-manifest.json`, отдельные result/log/
numeric/solution файлы с `-xy-metal-edges` и `-z-foil-surfaces`, prepared
`layered-paired-plan/manifest.json` и `refinement.json`. Исторические failures
и saved baseline сохранены. `constrained-paired-audit.png` визуально просмотрен;
interactive browser controls не проверялись. Script syntax и static artifact
assertions pass; broad native/addon/package checks не повторялись, production
source в этом checkpoint не менялся.

Следующий proposal для parent review: bounded postprocessing уже сохранённых
solution/assembly данных для распределения energy/defect по spatial regions и
source/far transition, без новых solves. Только после этого выбирать отдельный
mesh/domain control с pattern/native symbolic budget; новый refinement или solver
не разрешён этим checkpoint. L blockers §8.11, full quadratic magnetic gate,
real-board extraction/ports/recovery и три final source/DSL/export примера остаются
открытыми. P7, P8/P11 не закрыты; placementF32/GPU без изменений.


### 8.13. Saved-field localization и один nested transition control, 2026-10-24

Выполнено задание `next-task-extraction-capacitance-diagnostic.md`, скопированное
отдельным файлом из main150dbbe без merge и без переноса чужих изменений.
D8, D16 и оба paired fields §8.12 переиспользованы, их solves не повторялись.
`localize-constrained-field.py` восстанавливает full potentials из saved free-node
solution и exact full-metal/far traces, затем интегрирует local energy напрямую
по overlay polygons: W=.5 EPS0 integral |grad(phi)|² dV [J]. Common=[1,1] и
differential=[1,-1] — отдельные возбуждения. Regional sums совпали с
.5 vᵀCv: max relative error1.34e-13 <=1e-8 для всех пяти полей, включая новый
control. При exactly-zero full energy требуется exactly-zero regional sum;
иначе relative discrepancy считается infinite. Нулевой denominator не скрывается.

Регионы задаются последовательно, без пересечения, r=max(|x|,|y|), mm:
inner gap r<=.5,.035<z<1; edge/sidewall .5<=r<=.58,-.08<=z<=1.115;
coarse transition — оставшаяся часть r<=1.78,-.8<=z<=1.835; outer — остаток.
Energy polygons clipped геометрически, все z границы совпадают с saved planes;
tiny volume fragments сохранены. Полные regional numbers для D8/D16/paired и
**послойные** slab energies и z-face
indicators сохранены в `layered-localization/*.json`.

Local underresolution измерена interior displacement-flux jumps, не solved
algebraic residual и не величиной энергии. D=-EPS0 grad(phi) [C/m²],
eta_F²=.5 h_n/EPS0 integral_F [D dot n]² dS [J]. Для vertical facets h_n=min(2A/l)
соседних XY cells; для horizontal facets — harmonic adjacent slab heights.
Внутри constant-vacuum P1(xy)*P1(z) cells volume Laplacian равен нулю;
все interior XY/z faces учитываются через common refinement. Physical metal и
far boundaries не объявляются interior jump. Length<=1e-14mm numerical overlay
facets исключены только из flux diagnostic; energy fragments не исключаются.
Face region назначается segmented midpoint с source-priority tie на границе.
Это **anisotropic heuristic**, не certified error bound; высокая энергия сама
по себе не служит оценкой ошибки.

Ниже значения fJ, в каждой ячейке common / differential. Indicator — сумма
XY и z flux terms; не смешивать его с фактической ошибкой против reference.

| Region | saved D16 energy | new transition energy | saved D16 indicator | new transition indicator |
|---|---:|---:|---:|---:|
| inner-gap |2.172892 / 20.284417|2.087933 / 20.180165|0.342815 / 0.667893|0.328573 / 0.633602|
| metal-edge-sidewall |3.899861 / 13.066996|3.765605 / 12.680918|1.679719 / 4.841815|1.641522 / 4.696183|
| coarse-transition |19.514874 / 31.948393|19.462977 / 31.378685|7.762624 / 12.869114|4.517596 / 8.759308|
| outer-domain |10.260665 / 2.244640|9.976064 / 2.273423|4.640028 / 1.376906|4.361075 / 1.170340|

`diagnose-spatial-enrichment.py` показал: baseline XY flux indicator в кольце
.82..1.78mm составляет26.00% common /38.51% differential от полного XY indicator.
Весь coarse-transition regional XY indicator6.868/11.043fJ больше sidewall
1.581/4.617fJ. Ранее XY edge control снизил sidewall indicator, но почти не
обогатил transition; его новая triangulation **не вложена**:188/187 new triangles
BOTTOM/TOP пересекают old edges, хотя old vertices сохранены. Гарантированная
монотонность для того control не утверждается. z control вложен по z, но не
добавляет coarse XY функций. Поэтому малые paired changes не опровергали coarse
transition error.

| Hypothesis | Evidence / conclusion |
|---|---|
| Только near-face z spacing объясняет differential floor | z control почти не меняет coarse indicator и energy; недостаточно |
| Только segmentation metal edge объясняет floor | edge indicator падает, external differential остаётся5.1978%; недостаточно |
| Coarse XY transition .82..1.78mm даёт существенный вклад | высокий interior flux indicator и следующий различающий nested control поддерживают вклад |
| Remaining common error только от far boundary | D8/D16 trend совместим, но outer XY indicator тоже существенен; уникальная причина не установлена |

**До запуска** записан `layered-transition-plan/pre-solve-hypothesis.json`:
один параметр — midpoint enrichment64 shared interior edges старых annulus
triangles .82..1.78mm на каждой XY plane. Child triangles остаются внутри old
triangles; все old source/far edges, vertices,61zplanes,35um foil,965um gap,
vacuum и D16 box сохранены. Old affine reproduction max5.33e-15mm; nesting
construction даёт старое FE space как subspace нового (floating geometric
outside-parent roundoff отдельно сохранён). Это не general solver и не новый
PCB mesher. Actual prior pattern24461nodes/20961free/1635264local terms/419561nnz;
faer symbolic factor80343328bytes, scratch7069247bytes, planned native98152831bytes.
Conservative combined prediction240MiB; unchanged supervisor512MiB/60s mandatory.
Ожидание: differential energy drop>=.3% baseline и target XY indicator drop>=20%.
Refutation: energy drop<.1% **и** indicator drop<20%; intermediate=inconclusive.
Проверять full/common/differential против frozen FastCap32/o6 независимо.

После освобождения Thermal и короткого slot handoff от P6 выполнен **ровно один**
additional solve. Whole process4.8984s, sampled combined196599808bytes;
numeric CSC exact matched prior pattern. C11=50.895fF,C12=-15.610fF;
full error8.29822% **FAIL**, common8.29815% **FAIL**, differential3.70052% PASS.
Baseline-relative stored energy drops common1.55018% / differential1.52678%;
target XY indicator drops32.9409% /26.5573%. Both predeclared expectations met;
coarse transition установлен как **существенный contributor**, но не уникальная
причина и не convergence proof. Max free residual9.16e-16, energy2.89e-14,
charge1.20e-13 — numerical consistency; full accuracy остаётся FAIL.
Heavy slot сразу возвращён P6, follow-on solves не запускались.

Evidence: `layered-localization/manifest.json`, отдельный new-control localization,
`spatial-enrichment.json`, `layered-transition-plan/` pre-plan/pattern/native symbolic,
`layered-constrained-field/transition-manifest.json` и distinct raw/result/solution.
`localized-capacitance-audit.json` содержит source/input/output hashes, independent
modal errors, regional sum и predeclared hypothesis checks. Executed localization
v1 scripts сохранены с exact matching hashes. `localized-capacitance-audit.png`
визуально просмотрен: regional energy/indicator, radial target и external gates.
Affine potential patch на new XY cells: max gradient discrepancy4.01e-11V/m;
independent Gauss slab flux integral relative error2.23e-16. Browser controls
не проверялись. Syntax/static QA и git diff --check PASS;
production source/DSL не менялись, broad unchanged native/package checks не повторялись.

Остающийся blocker — full/common8.30% против5%; невозможно приписать остаток
только finite far box либо только remaining XY error по этому одному control.
Два варианта для следующего parent/architect decision, **не новые разрешённые solves**:

1. Отдельный domain-only D32 control на исходной XY geometry: ориентировочно
   369nodes/plane x65planes=23985nodes, прежний25k cap потенциально достижим;
   actual local-term/native symbolic/memory plan обязателен. Этот контроль отделит
   часть finite-box common effect, но сам не соединит его с новым transition space.
2. Совместить extended domain с локализованным по z transition enrichment:
   uniform combination433x65=28145nodes **не проходит** cap. Требуется новый
   bounded geometry/space plan с переменными XY planes и консервативным common
   refinement, не uniform cap increase; factor cost до actual symbolic неизвестен.
   Это design work, не разрешение нового FEM/BEM engine.

Production P7 **не принят**. L conditioning/cost, full magnetic quadratic gate,
real-board ports/recovery и три final source/DSL/export примера остаются открыты.
P8/P11 и placementF32/GPU без изменений; commits не создавались.


### 8.14. D32 domain-only и combined nested controls, 2026-10-24

Выполнено main75cd2d7 `next-task-extraction-domain-control.md`; скопирован только
этот task-файл. Явное решение архитектора подняло **только два диагностических**
node/local-term caps до35000/4000000. Production caps, independent5% accuracy,
physical scope, CPU/F64 и placementF32/GPU не менялись. Memory512MiB/wall60s
остались прежними. Выполнены ровно два новых solves, старые не повторялись.

Domain-only D32 добавил32mm square ring и zplanes -32,-24,25.035,33.035mm.
Все old D16 triangle coordinate tuples и61zplanes exact subset; source trace,
35um foil/965um gap, vacuum, оба full conductor voltages сохранены. Новый far-zero
box x/y±32,z=-32..33.035mm контролирует approximation к infinity reference;
ни один conductor не сделан hidden ground. Поля D8/D16 переиспользованы.
Перед solve actual faer symbolic factor79456328bytes,scratch6872063bytes;
полный conservative plan217747375bytes включал Python peak/native arrays/
numeric values/32MiB margin. Actual combined peak204640256bytes,4.5890s.

D32 baseline оставил differential5.30464%, но common10.0034→8.25084%: separate
finite-domain contribution подтверждён, spatial floor не исчез. По сохранённому
D32 field `n8-d32-zfine-radial.json` кольца .82..1.78 и1.78..4mm содержат вместе
56.96% common /55.92% differential XY jump indicator. В `combined-selection.json`
**до второго solve** выбраны sequential nested midpoint refinements64 shared
internal edges каждого из двух колец: +128XYnodes/plane, source/ring/far boundary
edges и все65zplanes неизменны. Второй шаг строится поверх первого, старые FE
spaces D32 baseline и D16 inner-transition сохраняются через old-triangle
subdivision; geometric containment roundoff max1.39e-16mm² сохранён отдельно.
Это прежние constrained coupon/trace/assembly и pinned faer, не новый mesher.

Первая combined preflight проверка отказала, требуя exact old triangles вместо
допустимого nested subdivision; её log сохранён. Исправлена именно проверка
FE-space containment, без изменения физических inputs и до numeric solve.
Actual combined pattern32305nodes/28485free/2188224terms/575381nnz;
symbolic factor144245384bytes,scratch9693311bytes, conservative full plan303117935bytes.
Actual combined peak279842816bytes,7.2368s; exact numeric CSC совпал с pattern.
Standalone wrapper собран против frozen isolated faer rlib (8.26s), только
его diagnostic35k/4M checks отличаются; production native crate не менялся.
Slot предоставлен P6 после addon build и сразу возвращён после двух solves.

| Control | full error | common error | differential error | wall / combined peak |
|---|---:|---:|---:|---:|
| saved D8 baseline |13.6364%|13.6363%|5.3359%|reuse|
| saved D16 baseline |10.0035%|10.0034%|5.3083%|reuse|
| D32 baseline |8.2510% FAIL|8.2508% FAIL|5.3046% FAIL|4.59s /204.64MB|
| saved D16 inner-transition |8.2982%|8.2981%|3.7005%|reuse|
| D32 combined two-annulus transition |5.5964% **FAIL**|5.5963% **FAIL**|3.5219% PASS|7.24s /279.84MB|

Нормы вычислены независимо против frozen FastCap plates32/o6, symmetric
quadratic part для energy и raw entries для scalar diagnostics. New full C:
C11=50.3976fF,C12=-15.9933fF. Separate baseline domain D16→D32 change:
full/common1.61898%, differential.0035273%. Separate D32 mesh-only change:
full/common2.51387%, differential1.72211%, нормирование на новое поле.
D16 inner-transition→D32 combined change2.55868% full включает **и domain,
и дополнительное outer-annulus refinement**; это не чистый domain control.
Adjacent changes не заменяют external5% gate. Max new free residual1.45e-15,
energy residual3.20e-14, charge2.88e-13 — consistency, не spatial accuracy.

Evidence: `layered-domain32-plan/{baseline,combined}/manifest.json`, `pre-solve.json`,
patterns/native symbolic, `combined-selection.json`; distinct D32 field result/
numeric/solution/log files и `domain32-{baseline,combined}-manifest.json`.
`domain32-capacitance-audit.json/.png` содержит independent norms, separate changes,
resource checks и input/source/output hashes. PNG визуально просмотрен; browser
controls не проверялись. Executed baseline planner source snapshot сохранён;
failed preflight/log и все прежние failed fields сохранены. Syntax/static
assertions и git diff --check PASS, unchanged broad builds/tests не повторялись.

Итог — concrete remaining blocker: common/full5.596% >5%, несмотря separate
domain/mesh improvement. Coupon ещё не доказывает требуемую пригодность, поэтому
новая production implementation не предлагается к запуску. До следующего решения
архитектора нужно отдельно закрыть remaining common accuracy/convergence; эти
два probes исчерпаны, новых solves/этапов не начато. Даже будущий coupon PASS
не закроет actual geometries, L conditioning/cost, recovery и public DSL.
P7/P8/P11 не приняты; commits не создавались. По указанию родителя работа
остановлена на этом сохранённом evidence packet, review отложен.


### 8.15. Final D64 domain/mesh controls, 2026-10-24

Выполнено mainf320bf9 `next-task-extraction-capacitance-final-controls.md`,
скопирован только task-файл. Ровно два новых controls; все прежние fields reuse,
failed history сохранена. Явное diagnostic разрешение50000nodes/6000000terms
не меняет production API/defaults. Memory512MiB/wall60s и accuracy5% unchanged.

**Domain only:** на сохранённом D32 combined space добавлены только XY64mm ring
и zplanes -64,-48,49.035,65.035mm. Все old D32 combined XY triangle coordinate
triples и65zplanes exact subsets; source constraints, coordinates бывшего
far box и internal FE space сохранены. Старый far boundary становится interior, новый far-zero box контролирует
infinity truncation. Оба conductor potentials остаются independent full traces,
foil35um/gap965um/vacuum/material/source geometry неизменны. Новый mesh36501nodes,
32489free,2483136local terms,658177nnz. Actual symbolic factor183390328bytes,
scratch11084351bytes; prior combined memory plan352100383bytes. Solve8.7723s,
sampled simultaneous Python/native peak336084992bytes.

Перед вторым solve сохранённый D64 flux audit показал: ещё не refined кольцо
.58..82mm содержит14.21% common /25.53% differential XY indicator. Выбран
**один nested mesh-only** control: midpoint functions64shared internal edges
только этого кольца; source/ring/far boundary edges, старые coordinates и все
69D64zplanes fixed. Old FE space сохраняется old-triangle subdivision; measured
containment roundoff max5.21e-18mm². Выбор/ожидаемые energy/indicator effects и
independent5%/separate2% gates записаны в `mesh-selection.json` до numeric solve.
Это не uniform refinement ради green. Uniform повторное деление всего уже
refined .82..1.78mm child annulus потребовало бы256extra nodes/plane и54165nodes,
выше50k cap; фактический selected control40917nodes/36777free/2796480terms/747329nnz.
Actual symbolic factor228062504bytes,scratch12586175bytes; prior simultaneous
memory plan408428367bytes. Solve10.1643s, sampled combined375234560bytes.
Target XY indicator после selected refinement упал42.89% common /38.63% differential.

Оба plans учитывали measured Python pattern working set, actual faer native
arrays, numeric-values delta и32MiB reserve; runtime supervisor проверял весь
Python/native process tree. Exact numeric CSC matched actual symbolic pattern.
Изолированный50k/6M wrapper переиспользовал frozen faer rlib и исходный solver;
production native source/limits не изменялись. Thermal предоставил два коротких
слота отдельно; каждый сразу возвращён, дальнейших heavy работ нет.

| Control | independent full error | common error | differential error | full controlled change |
|---|---:|---:|---:|---:|
| saved D32 combined |5.59643% FAIL|5.59628% FAIL|3.52188% PASS|reuse|
| D64 domain-only |4.77665% PASS|4.77641% PASS|3.52141% PASS|.78250% domain PASS<=2%|
| D64 mesh-only |4.41974% PASS|4.41954% PASS|2.89846% PASS|.60540% mesh PASS<=2%|

Common/differential domain changes=.78250/.00045036%; mesh changes=.34176/.60540%,
нормирование на новое поле. Full positive-range norm проверяет все quadratic
excitations, не только эти два modes. Reference — frozen FastCap plates32/o6;
raw unsymmetrized matrix/entry diagnostics сохранены, symmetric part используется
только для quadratic norm. Final C11=50.0060fF,C12=-15.9851fF; max scalar raw-entry
error3.4274%. Final free residual1.29e-15, energy residual5.78e-14, charge2.48e-13; consistency не подменяет accuracy.

**Текущие coupon gates PASS**: independent full/common/differential<=5% и два
отдельных domain/mesh changes<=2%. Это finite-vacuum two-plate diagnostic, не
сертификат asymptotic convergence или arbitrary PCB. Запас full accuracy до
5% только~.58percentage point. Dielectrics/layers/vias/multiple conductors,
real-board recovery/ports и public DSL этим не приняты; L conditioning/cost и
full magnetic quadratic gate остаются отдельными обязательствами.

Evidence: `layered-domain64-plan/{baseline,mesh}/manifest.json`, `pre-solve.json`,
actual symbolic/patterns и `mesh-selection.json`; отдельные D64 raw/numeric/
solution/result logs под `layered-constrained-field/`; saved radial indicator
outputs. `domain64-capacitance-audit.json/.png` включает raw reference, scalar и
modal errors, separate changes и source/input/output hashes. PNG просмотрен;
browser controls не проверялись. Syntax/static assertions и git diff --check PASS;
неизменные broad native/addon/package tests не повторялись.

Следующее действие предложено в [native port proposal](../design/next-proposal-layered-capacitance-native-port.md):
минимальный внутренний Rust coupon port product-space assembly с reuse Spade CDT,
source constraints и faer budget, затем новые independent dielectric/layer/via/
full-matrix coupons. Текущий native tetrahedral primitive не объявлен эквивалентно
проверенным product-space coupon. Proposal не запущен; implementation, новые
solves/этапы/agents/commits не начаты. P7 production **не принят**, P8/P11 не
закрыты. Остановлено на сохранённом evidence packet до следующего review.

### 8.16. Internal native layered capacitance: bounded coupon port

Architect accepted §8.15/proposal and issued
`next-task-layered-capacitance-native.md` (copied alone from MAIN). Implemented
`native/analyze/src/extraction/local/layered_capacitance.rs`: CPU/F64
P1(XY)*P1(z), exact frozen and pinned Spade CDT paths, conservative F64 convex
triangle intersections, exact degree2 XY moments and analytic piecewise-epsilon
z integrals. Source identities/coordinates, finite35um copper exclusion and
face/sidewall traces remain explicit. Unsupported partial metal-cell ownership,
invalid embedding/material/source/boundary cases refuse. No public analyzer,
DSL, board adapter, via or P6 type change; no placement/GPU default changes.

Typed16byte COO sorts/collapses in place into CSC; full/free matrices, geometry,
cell capacity, RHS/basis/recovery and actual symbolic faer factor/scratch are
admitted simultaneously. Pre-symbolic working/index envelope is included.
Every request explicitly passes50k/6M/512MiB/60s diagnostic budgets; production
defaults are untouched. One original Instant covers CDT/assembly/symbolic/
factor/solve/recovery, including later recover() calls. Opaque faer phases are
checked before/after; external process supervision supplies the hard wall/RSS
cap for evidence. Full conductor potentials/C/charges and dielectric centroid
fields are retained, with independent Gauss field-energy recovery. Explicit
insulated analytic coupons preserve gauge/all conductors; infinity coupons have
an explicit far-zero box, never a hidden conductor ground.

**Focused Rust gates:16 PASS.** Four layered integration tests cover analytic
asymmetric epsilon2/5/3 three-conductor series C, two distinct cross-layer
triangulations, native CDT constraint retention, fully prescribed free0 case,
allocation/deadline/geometry negatives. Eleven unchanged local primitives pass
after shared planar validator reuse. One independent affine/constant/material
overlap patch passes: exact area2.5e-7m²; legacy integer-grid BooleanOps gave
2.5000000010186343e-7 and previously failed the unchanged1e-10 conservation gate.
Native overlay now returns2.5e-7 without snapping/dropping positive overlaps.
Initial failed overlay source/log and unsorted offline z-extension input refusal
are preserved; generator now sorts the complete plane set before association.

**Bounded real-source optimized native results** (full quadratic comparisons,
raw independent matrices retained; only quadratic forms use symmetric parts):

| Coupon | Nodes | Native full error | Exact Python parity | Wall s | Peak bytes |
|---|---:|---:|---:|---:|---:|
| Failed D32 retained |32305|5.59643% FAIL|1.071e-14|2.188|200916992|
| D64 domain |36501|4.77665%|2.914e-14|2.599|227405824|
| D64 mesh |40917|4.41974%|4.920e-14|3.835|296710144|
| Different native CDT |40917|4.43925%|not same mesh|3.669|282722304|
| Shifted final |40917|4.44064%|new coupon|3.397|256118784|
| Three-conductor final |49941|4.90099%|new coupon|6.992|381059072|

The harness includes the actual Rust module and pinned cached dependency rlibs,
not a replacement solver; source/compiler/flags/rlib/binary hashes are saved.
No profile/default/dependency or shared addon rebuild was necessary for this
additive internal library. Native retains tiny positive overlap entries instead
of Python's old removal of numerical-zero intersections: final2796912 local
terms versus old2796480, with full parity4.92e-14. Actual factor/scratch and
simultaneous plan for final two-foil:238269376/12585279/348648295bytes;
three-foil:310377688/15300479/442106756bytes. All requests stay under512MiB/60s.
Worst saved free/charge/work/recovery errors are below2.27e-12/1e-9 gate.
Algebraic residuals are not used as physical accuracy evidence.

Shifted foil: top metal and near source rings≤4mm displaced+.25mm in x; outer
domain rings retained. Separate D32coarse→D64coarse full domain change.78578%,
then nested annulus mesh change.60677%; raw full errors5.62014% FAIL,
4.79677%,4.44064%. Three finite foils at0/1/2mm: D32/D64/coarse-refined raw
full errors5.99775% FAIL,4.95010%,4.90099%; separate domain1.00151% and
mesh.058989%. Three-foil mesh control adds only8 longest existing shared
interior edges/plane in predeclared .58..82mm high-flux annulus (49,941nodes),
not a global convergence certificate. Frozen two-foil separate changes reproduce
.78250%domain/.60540%mesh. No mesh/domain/source changes were mixed to claim
convergence. No further refinement or cap increases were attempted.

**Uncertainties remain separate.** Source polygons, z/material interfaces,
foil thickness/shifts and constraints are exact authored coupon definitions;
finite extent/infinity-box assumptions remain explicit. Offline primary
FastCap2 source04eee66446be0629f0607e68f369acad1072dc4d, pinned binary hash and
raw stdout/stderr/unsymmetrized C are tracked for vacuum shifted/three foils.
Each n16/n32 reference stayed≤4.584s/333864960bytes; observed full reference
changes.15484%/.17802% meet predeclared1% but are not certified continuum bounds.
Analytic asymmetric dielectric reference uses series electrostatics, not vacuum
FastCap. Three-foil remaining5% margin.09901percentage point is smaller than
its observed.17802% reference change: additive screening gives5.07901%.
Thus raw coupon gates pass, but an uncertainty-inclusive5% claim for three
foils is **not established**. This limitation blocks broader acceptance; it is
not hidden by reciprocity, small mesh changes or the raw4.90099% result.

Tracked fixtures/generators/reference/native summary and offline build/run/
audit/plot tools: `tests/fixtures/analysis-local-extraction/layered-native/`.
Saved detailed packet: `debugging/p7-design-checkpoint/layered-native-audit.json`
and `.png`, `layered-native-probe-build.json`, `layered-native-runs/*/run.json`,
raw stdout/stderr and previous failures. PNG visually inspected: full errors,
observed reference changes, planned/measured memory, sampled native fields and
full three-conductor C; no browser controls. Diff/syntax/static/hash QA recorded.
Heavy slot coordinated/returned P6; no new agents or commits. STOP for parent
review. P7 production/P8/P11 remain unaccepted; real-board/DSL/vias and existing
L conditioning/cancellation/cost/full magnetic energy blockers remain open.

### 8.17. Three-conductor reference margin: one higher offline reference

MAIN805f183 task `next-task-capacitance-reference-margin.md` copied alone.
Previous §8.16 native matrices/source/binary are revalidated and reused;
no kernel/default/geometry/domain/mesh changes, no native solve/test reruns.
Earlier raw n16/n32 references and all failed accuracy/margin screens remain.

Preflight selected **one n48 reference,41472panels**, before launch, with the
same original `generate-reference.py::panels()` finite three-foil geometry:
1mm squares,35um thickness, z0/1/2mm, vacuum. Authoritative FastCap2 source and
pinned console binary are unchanged (source commit04eee66446be0629f0607e68f369acad1072dc4d).
Measured n16/n32 resource scaling predicted affine-panel memory758975147bytes;
×1.25 safety envelope948718933bytes is below the expressly allowed offline
1024MiB. Quadratic-panel time×1.5 estimated34.807s; n64 memory estimate alone
exceeds1GiB, so n48 was fixed. External process wall/RSS supervision enforced
1024MiB/60s. Actual **39.820943s/790548480bytes**, exit0: wall estimate was
optimistic, but authorized hard limits passed. Native/production512MiB/60s
was not raised. No extra reference attempt or conditional native control ran.

Positive-range full quadratic norm uses symmetric parts of raw matrices only
for the quadratic form; all raw unsymmetrized entries remain in the audit.
Each reference symmetric part is strictly positive definite.

| Reference | Saved native full error | Max raw-entry error | Last observed reference change | Error +2×last change |
|---|---:|---:|---:|---:|
|n16|5.008875% FAIL|3.960381%|—|—|
|n32|4.900994%|3.801020%|.178017%|5.257028% FAIL|
|n48|4.880861%|3.772846%|.034255%|**4.949371% PASS**|

Reference16→48 full change.211879%; sequential16→32 and32→48 changes strictly
decrease by a factor~5.20. New raw-reference reciprocity5.22036e-6 is recorded,
not silently repaired for scalar comparisons. Every signed9 raw-entry error
and positive eigenvalue is saved. Native free/charge/work/recovery residuals
remain3.4584e-15/2.1349e-12/2.3603e-13/4.5739e-14; these are reused consistency
gates, not new accuracy evidence. Prior geometry, isolated domain1.00151% and
limited mesh.058989% changes are unchanged and retain their stated limitations.

**Architect's conservative working screen PASS:**4.880861%+2×.034255%=
4.949371%≤5%, with observed reference changes strictly decreasing. Its remaining
margin is only.050629percentage point. Observed reference changes and this
screen are **not certified continuum error bounds**. Source geometry/material,
domain truncation, limited mesh control and algebraic residuals stay separate.
The conditional60k/7M/768MiB native control was not needed and was not run;
none of those diagnostic allowances became production defaults.

Tracked reference/preflight/driver/provenance/raw matrix/stdout/stderr and compact
audit: `tests/fixtures/analysis-local-extraction/layered-native/references/`.
Offline `refine-three-reference.py`, `audit-reference-margin.py` and
`plot-reference-margin.py` reproduce preflight/audit/saved-value figure, without
runtime dependencies. Detailed packet:
`debugging/p7-design-checkpoint/three-reference-margin/{run.json,audit.json,audit.png,manifest.json}`.
PNG visually inspected; no browser controls. Static/Python syntax/diff/hash QA
recorded. P6 granted/received the single reference heavy slot. No source changes
required native tests/addon rebuild; no commits/agents/new stages. STOP for
parent code review. This closes the requested working margin screen only;
P7 production, public PCB/DSL/vias and L blockers remain unaccepted.
