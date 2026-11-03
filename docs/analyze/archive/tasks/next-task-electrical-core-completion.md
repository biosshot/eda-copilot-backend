# Остаток P6: завершение общего электрического ядра

Дата: 2026-10-23. База: 096d96e, принятые P6a/P6b и P7a/P7b. Выполнять вместе с [правилами параллельной работы](../design/parallel-p6-p7-execution.md). Цель — завершить оставшиеся возможности P6, не реализовывать заново принятую RLC-математику и не объявлять этим готовыми P8/P11.

## 1. Что уже работает и должно сохраниться

- Explicit R/C/L, независимые идеальные источники, circuit DC, driving-point impedance и adaptive transient/continuation.
- P6b S/Z/Y/Touchstone, соединение сетей, нормировка портов, проверки полоса/пассивность в принятой области.
- a.model, существующие параметры DSL, bounded worker/report, измерения с единицами/validity.
- PCB FEM DC, geometry, PEC/lossy cross sections, placement F32/GPU.

## 2. Обязательные результаты направления

### E1. Полнота электрической постановки и моделей

Проаудировать пункты P6 общего плана против кода и записать матрицу «принято / нужно реализовать / область применения». Исполнить согласованные простые driver/receiver и необходимые supply/load/R/C/L модели через общее ядро; использовать существующие low/high/outputResistance, input C/R и stimulus-описания. Явно определить pin/port mapping, reference, units, начальные условия и поведение неподдержанной модели. Не вводить полный SPICE, произвольные нелинейные компоненты, IBIS-AMI, автоматический datasheet parsing или неограниченное семейство controlled sources.

Проверить предусмотренное разрешение параметров участвующих компонентов: явный DSL имеет приоритет, optional UUID/данные платы не обязательны. Не требовать весь BOM и не угадывать отсутствующие физические параметры. Возможность создать описание модели не считать её численной реализацией.

### E2. Частотный отклик явно заданной сети

Расширить существующую MNA-основу для необходимых заданных возбуждений/передачи и наблюдаемых напряжений/токов, сохраняя semantics отдельного тестового impedance excitation. Зафиксировать, как выражаются амплитуды/фазы через согласованный DSL: не трактовать DC constant или transient waveform как AC amplitude неявно. Если текущий контракт не выражает необходимое условие, до кода предложить минимальное дополнение архитектору.

Результат: собственные частотные отклики RLC-системы подтверждены независимыми комплексными ответами, а не только совпадением с тем же solver. Не подменять схему импортированным golden S-resource.

### E3. Временной отклик частотных/распределённых моделей

Реализовать проверенный путь от поддержанной частотной/распределённой модели к временному отклику вместе с явными возбуждениями и нагрузками. Выбор метода (state-space/rational fitting/convolution/допустимые аналитические линии) обосновать в design addendum до большой реализации. Не требуется реализовать все названия алгоритмов из плана.

Обязательны: согласованная нормировка и знаки портов; обращение с delay; DC/high-frequency границы; начальные состояния; устойчивость и причинность; сохранение пассивности там, где исходная модель пассивна; проверка ошибки модели отдельно от временной дискретизации; запрет неявной экстраполяции. Активная RF-модель не обязана быть пассивной, но её устойчивость и допустимое подключение проверяются.

Недостаточная полоса, неизвестный DC, плохой fit, неподдержанные reference/геометрия или несовместимые начальные условия дают явный отказ/непригодный результат. Высокочастотная weak-loss P7b линия не превращается автоматически в broadband/DC модель. Путь должен иметь реально поддержанные положительные случаи; одни отказы не закрывают E3.

### E4. Общий стык с P7

Согласовать с исполнителем P7 необходимую форму coupled/passive модели до изменений общих типов. Проверить независимым small-network тестом import/solve/port mapping/frequency/time по одному и тому же физическому случаю. Если пригодная модель уже представима существующими типами, новые типы не добавлять. Не переносить геометрический extractor в P6.

## 3. Границы

- Численные алгоритмы — Rust CPU/F64; TS разрешает ресурсы, собирает задачу и создаёт accessors. Использовать существующий addon/worker.
- Не реализовывать полный PCB PI, автоматическое извлечение трасс, board SI, TDR/eye/timing, EMI, Thermal/electrothermal, solve/optimize или общий cache framework.
- Для публичной демонстрации использовать существующие операции настолько, насколько они выражают задачу. Не добавлять новый публичный анализатор только ради теста внутреннего ядра. Не объявлять весь SI готовым, если для узкого пути используется его принятая конфигурация.
- Сначала E1/E2 и design E3/E4; после проверки архитектора закончить E3/E4. Это этапы одного поручения, не повод остановиться после документации или первого маленького теста.

## 4. Проверки и критерии приёмки

1. Независимые RC/RLC transfer/phase/transient; нагруженный linear driver/RC receiver; согласованная и отражающая линия с известной задержкой; пассивный attenuator и каскад; multiport coupling с проверкой всех существенных портов, а не только S21.
2. Согласованность частотного и временного отклика в заявленной полосе; refinement времени/полосы/model order отдельно. До реализации численного oracle записать допуски и основания; не ослаблять их для прошедшего результата.
3. Негативные cases: неполные модели/порты/reference, недостаточная полоса и DC, noncausal/unstable/passivity нарушения в применимых условиях, плохое приближение, impulses/начальные условия, budget/cancel/timeout и успешный повтор.
4. Сохранение KCL/мощности/энергии в применимых системах. Скаляры/серии всегда с units/validity; никаких NaN, фиктивных нулей или зелёных checks по неполным данным.
5. Не менее трёх прямых DSL-сценариев, включая standalone и сценарии в контексте сохранённых плат. Привязка к плате честная: явно заданная электрическая сеть не называется автоматически извлечённой PCB. Графики вход/выход, frequency/phase, ошибки/сходимость, port/model mapping и допущения. Хотя бы один ожидаемый failed check с продолжением выполнения.
6. Независимый audit сохранённых результатов и отрицательные тесты самого audit. Реальные численные массивы и все нужные случаи сохраняются; report не урезается ради успешного audit. Из-за текущего запрета browser automation HTML controls не считать проверенными; static PNG из численных данных допустимы с явным описанием ограничения.
7. Focused Rust/Node, native build, typecheck/build, установленный пакет и релевантные регрессии. Тяжёлый слот согласовать. Записать команды/exit codes, source/addon/result hashes, ресурсы, границы и реальные visual checks в electrical-core-completion-verification.md.
8. Обновить точные статусы P6/DSL/docs/CHANGELOG. Не отмечать P6 полностью закрытым, пока не сопоставлены все его исходные пункты; остаток явно предъявить архитектору, а не скрывать общим unsupported.

## 5. Первая контрольная точка

Исполнитель дописывает design addendum: аудит E1–E4, минимальная модель между P6/P7, выбранный путь E3 с первичными источниками и сохранёнными эталонами, точные критерии/ограничения. Передаёт архитектору на проверку; независимую инвентаризацию и малые эксперименты можно продолжать. После согласования реализует, тестирует и исправляет замечания. Без самостоятельного коммита и перехода к P8/P11.

## 6. Design addendum исполнителя — checkpoint до production-кода

Дата: 2026-10-23. Worktree `analyze-p6-completion`, branch
`codex/analyze-p6-completion`, база `c8b6617`. Это предложение к проверке
архитектором; перечисленные ниже новые возможности ещё не реализованы.
Обычный addon сейчас занят Thermal; здесь не запускались сборки или корпуса.

### 6.1. Матрица исходного P6 против текущего кода

| Пункт исходного P6 | Уже принято / точка кода | Остаток этого поручения и область |
| --- | --- | --- |
| MNA, operating point, AC, transient | `electrical/network.rs`: sparse G/E; `solve.rs`: DC, отдельные 1 A Z-excitations, adaptive BE/continuation | E2: собственный AC response заданных источников, V/I и transfer. Сохранить отдельную семантику Z. Произвольные nonlinear/controlled sources не добавлять |
| R/C/L, supply/load | `CircuitElement` содержит R/C/L и идеальные V/I sources; local R/C/L численно раскрывает `electrical-executor.ts` | E1: согласовать существующие `Source`/`PortLoad` с явно заданной circuit, включая finite driver output R и receiver RC. Supply — явный источник, load — R/I, без нового класса converter |
| Driver/receiver/stimulus | `linear-driver`, `rc-input`, Driver/Receiver/Stimulus валидируются, но исполнителя нет; `executor.ts` отклоняет SI | E1/E3: исполнить существующий `.si().network(...)` только для заданных поддержанных ресурсов. `.channels(...)` и PCB SI остаются отказом. Bits/PRBS/waveform воспроизводимы, без protocol simulation |
| Разрешение участвующих компонентов | V2 содержит optional `value`/`partUuid`; `componentDescription()` сам не разрешает номиналы; RLC adapter принимает только готовые local модели | E1: явный DSL/model выше единственного board-instance value, затем точного заранее предоставленного model resource по PartUuid; никакого требования полного BOM. Недостающие parasitics не угадывать |
| S/Z/Y, connect, references, Touchstone | `electrical/frequency/*`, `frequency-model.ts`, `frequency-executor.ts`; common-return, complex interpolation, pointwise passivity | Сохранить. Дополнить двумя явно аналитическими data-форматами, описанными ниже. Finite-band samples не становятся временной моделью |
| Причинность/устойчивость по операции | P6b не доказывает их по samples и это документирует | E3: structural passive descriptor / causal delayed scattering. Для arbitrary active sampled RF time остаётся отказом: стабильность по таблице не установлена |
| Временной отклик frequency/distributed model, convolution, fitting по потребности | Пока отсутствует; `result.network` содержит лишь вычисленную сетку частот | E3: exact finite-dimensional descriptor и delayed-wave convolution, с независимой проверкой model error и temporal error. Общий black-box vector fitting и FFT не нужны выбранному методу; весь пункт нельзя описывать как универсальный Touchstone-to-time |
| Coupled R/L/C, P7 interchange | Scalar Element.nodes не выражает mutual matrices или general incidence | E4: общий coupled descriptor; P7 хранит геометрию/recovery, P6 решает матрицы. Без geometry в electrical solver |

Исходный checkbox про FFT/свёртки трактуется здесь как обязанность поддержать
временной отклик подтверждённым методом, а не написать все альтернативные
алгоритмы. Если архитектор требует именно отдельную FFT-операцию, её публичный
вход/выход надо определить до кода; добавлять новую операцию или выдавать
неиспользуемую FFT за готовый продукт нельзя. Точное состояние P6 после приёмки
описывается этой матрицей и проверками, без обещания произвольного SPICE/Touchstone.

### 6.2. E1/E2: существующий DSL и точные возбуждения

Для AC уже есть `Source.voltageSpectrum/currentSpectrum` с комплексными V/A
и явной осью Hz. Поэтому новый `acAmplitude`, отдельный AC analyzer или
преобразование DC constant/waveform в phasor **не предлагаются**. Сейчас
`prepare.ts` запрещает сочетание standalone circuit с `.sources/.loads`;
предлагается минимальное расширение исполнения уже имеющейся формы:

```js
const c = a.circuit({
  nodes: ['in', 'out', 'gnd'], reference: 'gnd',
  ports: [{ id: 'in', positive: 'in', negative: 'gnd' },
          { id: 'out', positive: 'out', negative: 'gnd' }],
  elements: [
    { id: 'R', kind: 'resistor', nodes: ['in', 'out'], resistanceOhm: 1000 },
    { id: 'C', kind: 'capacitor', nodes: ['out', 'gnd'],
      capacitanceF: 1e-9, ideal: true },
  ],
});
const f = [1e3, 2e3, 3e3, 4e3];
const ac = await a.pi().circuit(c).domain('frequency')
  .sources([{ port: c.port('in'), model: 'ideal', voltageSpectrum: {
    frequencyHz: f, values: f.map(() => ({ real: 1, imaginary: 0 })), unit: 'V',
  } }]).frequency({ startHz: 1e3, stopHz: 4e3, points: 4, scale: 'linear' }).run();
a.plot({ title: 'RC transfer', data: ac.transfer({ from: c.port('in'), to: c.port('out') }).magnitude() });
```

Это proposed runnable fixture после расширения, не утверждение текущей готовности.
Spectrum обязан точно покрывать requested frequencies; интерполяция/добавление
DC не выполняются. Phasor — peak complex amplitude, временная конвенция
`Re(V exp(+jωt))`. Направления V/I остаются first-minus-second / first-to-second.
Заданная Source добавляет настоящий источник/конечный выходной R; PortLoad —
настоящую нагрузку. Внутренние circuit sources имеют прежние DC/time значения и
обнуляются в AC. Дублирующие/противоречивые voltage constraints отвергаются,
не заменяются молча источниками сцены. В frequency ответе отдельно сохраняются
`excitationResponse` и независимые impedance-test RHS, с factor reuse.

`transfer({from,to})` означает `V(to)/V(from)` при единственном активном
независимом возбуждении этой постановки, не S21. Несколько активных источников,
нулевой/неразрешимый знаменатель дают unavailable transfer, но сохраняют
валидные отдельно вычисленные V/I. Активный источник и его port mapping
записаны в provenance; passive port не становится matched termination.

Для E1 переиспользуются `linear-driver` (идеальный authored voltage за явным
положительным Rout) и `rc-input` (C и optional shunt R). Driver low/high и bits
задают абсолютное напряжение; waveform задаёт абсолютную форму, не прибавку к
DC. Фронты проверяются на совместимость с bit interval; соседние одинаковые
биты не дают событий. Начало — первый бит при t=0; после конечной
последовательности сохраняется последний уровень. PRBS содержит явный nonzero
seed, документированные polynomial/taps и опубликованные первые биты; неизвестный
order отклоняется, а не выбирает случайный polynomial. Нулевой фронт допустим
только там, где прежняя impulse-совместимость MNA это разрешает.

Драйверы/приёмники численно исполняются через уже согласованный
`.si().network(...).drivers(...).receivers(...).time(...)`; лишь supplied network,
без `.channels`, extraction, timing/eye/TDR. Single-port local models используют
`port`; неполный/multiport mapping без подходящей модели отвергается.

Противоречие для component resolution: `CircuitElement.model` принимает Model,
а глобальные `resistor()/capacitor()/inductor()` возвращают ComponentDescription. Предлагаемый
минимальный вариант на ревью — разрешить ComponentDescription также в существующем
`kind:'model'` элементе, с обязательным явным `pins:{p,n}`. Альтернатива —
оставить circuit только полностью явным, разрешать nominal lookup лишь в board
операциях; тогда исходный P6 item нельзя закрыть для participating circuit
components. Новый factory/PartUuid field в RoutingBoardV2 не нужен.
Resolved report сохраняет исходный descriptor, точный board designator/PartUuid,
авторские/разрешённые значения и источник каждого параметра. Только nominal
R/C/L можно читать из строгого однозначного value; ESR/ESL/DCR остаются authored
или explicit ideal:true. Board value не переопределяет явный DSL. Неподдержанная
единица/кодировка/отсутствующий resource даёт адресный отказ. Existing TS device
resolver применим только на host side с сохранением resolved resource; solver
не запускает сетевой поиск. Здесь достаточно заранее предоставленного catalog,
UUID/доступ к сети не становятся обязательными.

### 6.3. E3/E4: минимальная coupled-модель, предложенная P7

С исполнителем P7 `/root/geometry_validation` согласованы математические
требования: general sparse incidence A, полные symmetric R/L branch matrices,
symmetric nodal C, явные named ports/reference и отдельный recovery current basis.
Concrete small matrix от P7 ещё ожидается; final DTO не принят.

Предлагается analytic Resource.data `format:'coupled-rlc'` через существующие
`a.model/a.network`, а не новая analysis factory. Его содержание: ordered
nonreference node IDs, reference ID, ordered branch IDs, sparse dimensionless A,
R[Ohm]/L[H]/C[F], named differential port incidence, source/model identity,
validity/analyses и applicability provenance. Допускается general incidence,
не только unit pairwise edges. Внутренние nodes/basis принадлежат экземпляру;
порты не соединяются по совпадению имени. Размеры, симметрия, PSD и gauge
проверяются до factor allocation; numerical failure не исправляется gmin.

Уравнения с током, положительным **внутрь** модели:

```
C dv/dt + A i = B ip
L di/dt + R i - A^T v = 0
vp = B^T v
E = diag(C,L),  G = [[0,A],[-A^T,R]]
W = (v^T C v + i^T L i)/2
vp^T ip = dW/dt + i^T R i
```

R,L,C должны быть PSD в их физических базисах, нулевые C rows и singular E
допустимы. Полный constrained pencil должен быть regular и поддерживать прежний
index-1 initial/event solve; лишняя gauge/вырожденные constraints отвергаются.
PSD проверяется на scale-normalized matrix, без исправления отрицательных
eigenvalues проекцией. Эта форма сохраняет reciprocity/passivity структурно;
неподдержанный active/nonreciprocal descriptor отклоняется. Existing active
S/Z/Y frequency cases сохраняются. Recovery matrices/current-basis geometry
хранит P7 и применяет к опубликованным stable branch/node IDs; P6 не читает PCB.

Компиляция использует тот же sparse G/E и incremental BE, DC — Gx=b,
frequency — (G+jωE)x=b. General mutual не превращается в независимые scalar L/C.
Reduction/fitting не требуются для первого обмена и не скрываются под именем
полной модели: сохраняются исходные физические degrees of freedom. Первые
лимиты не повышаются: existing 1024 nodes / 2048 elements / 128 ports / bounded
outputs; для matrix model отдельно admission по n+m, sparse nonzeros, symbolic
fill и existing memory budget. Dense validation только явно bounded small blocks;
большая модель должна получить диагностированный отказ до O(n²) allocation.

Frequency samples из P7 остаются частотными. Если extraction объявляет только
finite-band applicability и не подтверждает time applicability, его Resource
`analyses` не допускает SI/time; P6 не дописывает DC/high-frequency поведение.
Interchange-test времени использует общий **аналитический** coupled coupon,
где constitutive R/L/C — сами заданные идеальные уравнения, а не выдаёт этим
временную пригодность произвольной извлечённой платы. Извлечённые RLC и их
физическая полоса видны отдельно от математической модели идеального coupon.

### 6.4. Выбранный distributed/time путь и отвергнутые сокращения

Второй минимальный data-format на ревью — `delayed-scattering`: положительные
real port references, одна real constant scattering matrix K и общий delay τ>=0,
`b(t)=K a(t-τ)`. Он покрывает exact matched lossless two-port, его отражения при
явных нагрузках, статический passive attenuator (τ=0) и их `a.connect` каскады.
Для passive модели largest singular value K <=1 с численным guard; K не
перенормируется для прохождения. Этот класс аналитический на DC/high-frequency;
таблица конечной полосы не распознаётся эвристически как такой ресурс.
Frequency S(ω)=K exp(-jωτ) вычисляется тем же Resource с прежней power-wave
нормировкой `a=(V+RI)/(2 sqrt(R))`, `b=(V-RI)/(2 sqrt(R))`.

Это causal convolution с delayed delta kernel, без аппроксимации delay
полиномом/Pade, без periodization/IFFT. При τ=0 feedthrough входит в общую
алгебраическую систему и проверяется её conditioning. При τ>0 history хранится
в bounded ring; internal h<=min positive τ, history используется только с
принятых шагов, кусочно-линейно. События отражений/источников доставляются с
точным переносом τ; output sampleStep не задаёт internal grid. Step-doubling
оценивает отдельно waveform/history interpolation error. Переполнение history,
неразрешимый delay/event time, delay loop без однозначного initial state дают
отказ. До t=0 только явно определённая constant steady history из источников
t=0; неизвестные ранее travelling waves не заменяются нулём. В первом public
SI subset initial='steady' фиксирован semantics и отражён в отчёте; arbitrary
initial waves/continuation пока не выражены SiConfiguration и не обещаются.

Разделять три ошибки: exact resource-vs-oracle (без fit), numeric history/time
refinement и физическую applicability исходной модели. Для delayed model нет
конечного frequency cutoff и нет скрытой экстраполяции; для supplied finite-band
Touchstone, неизвестного DC, P7b skin-effect samples и noncausal sampled models
time отклоняется. General vector fitting не является безопасной заменой этой
проверки: устойчивые poles сами по себе не гарантируют пассивность. Если нужен
time произвольной finite table, потребуется отдельный согласованный fit/error/
causality/passivity/DC contract; это явный оставшийся предел класса, не выполненная
возможность. FFT/window/zero-padding не используются для выдумывания missing band.

Первичные источники метода (только математические references; код/таблицы
сторонних solvers не копируются, runtime dependency не добавляется):

- [Ho, Ruehli, Brennan, 1975, Modified Nodal Approach](https://spinningnumbers.org/assets/MNA75.pdf): MNA constraints/source currents; выбранное расширение остаётся в существующем G/E compiler.
- [Odabasioglu, Celik, Pileggi, PRIMA, 1998](https://bioee.ee.columbia.edu/courses/ee6321/papers/00712097.pdf): passive RLC descriptor/congruence structure; здесь берётся исходная full model, reduction не заявляется.
- [Modelica Standard Library, Lines.TLine1, primary implementation documentation](https://simulationresearch.lbl.gov/modelica/releases/msl/Modelica%203.2.1/help/Modelica_Electrical_Analog_Lines.html): exact lossless delay representation и его параметры; там приведена библиографическая ссылка Branin, Proc. IEEE 55 (1967), 2012–2013. Сам original Branin paper ещё не верифицирован; ранее найденный `agenda.infn/.../JPA_simulazioni_SP.pdf` не считается проверенным первичным PDF.
- [Gustavsen, Semlyen, 1999, Vector Fitting](https://www.sintef.no/globalassets/project/vectfit/vf_paper.pdf) и [2001, Passivity Enforcement](https://www.sintef.no/globalassets/project/vectfit/references/passivity.pdf): рассмотренная альтернатива fitting; не принимается shortcut stable-poles-only.

### 6.5. Независимые references, допуски и direct scenarios

Все числа ниже задаются **до** solver/ожидаемых результатов; текущие guards не
ослабляются. Relative сравнения имеют отдельно указанную абсолютную норму возле
нулей; фаза не проверяется там, где complex magnitude неразрешима.

| Gate | Oracle / наблюдение | Заранее заданный предел |
| --- | --- | --- |
| RC/RLC AC transfer и I | Независимые complex closed forms: H_RC=1/(1+sRC), H_RLC=1/(1+sRC+s²LC); finite-Rout divider | complex error <=1e-9 relative +1e-12 absolute; phase <=1e-6 deg вне nulls |
| Linear driver + RC receiver | Finite-front convolution/exponential, unloaded/loaded low/high | <=0.1% voltage swing +1e-6 V; temporal last-pair <=0.05% swing, output cadence отдельно |
| Coupled two-branch RLC | A=I, R=diag(2,3) Ohm, L=[[2,0.5],[0.5,1]] uH, C=diag(1,2) nF, two single-node ports to reference; all off-diagonal responses; independent 2x2 elimination / modal analytic transient | frequency <=1e-9 relative +1e-12 absolute; transient <=0.1% swing +1e-6 V; permute nodes/ports/current basis |
| Matched delay | Z0=50 Ohm, τ=2 ns, Rs=RL=50 Ohm: Vout(t)=Vin(t-τ)/2 | max waveform error <=0.1% swing +1e-6 V; edge delay <=0.1% τ +1 ps; no output before arrival beyond absolute bound |
| Reflections | Same line, RL=100 Ohm / open; independent bounce series Gamma=(R-Z0)/(R+Z0), finite ramp | same waveform/time guard; several round trips, both port V/I and initial history |
| Passive attenuator + cascade | K=[[0,0.5],[0.5,0]], τ=0; cascade with delayed thru and analytic loaded S equations | full matrix <=1e-10 absolute; wave power / delay history energy ledger |
| Conservation | Original-equation KCL/residual, complex power, stored E and dissipated/source/delay energy | unchanged row residual <=1e-8; power <=1e-12 W +1e-8 exchange; transient energy error decreases on refinement and <=0.1% exchanged energy +1e-12 J on references |
| Model/time separation | Delayed exact resource versus complex exp; history time tolerance refinement independently | frequency model error <=1e-10 absolute, final time last-pair <=0.05% swing; changing sampleStep cannot repair a failed internal gate |

Для этого coupled coupon independent AC oracle:
`Y(s)=sC+(R+sL)^(-1)`, `Z(s)=Y(s)^(-1)` через явную 2x2 формулу.
Transient oracle для current step j после нулевого initial state:
`xdot=F x+[C^(-1)j,0]`,
`F=[[0,-C^(-1)],[L^(-1),-L^(-1)R]]`,
`x=[v,i]`, `x_inf=[Rj,j]`, `x(t)=x_inf-exp(Ft)x_inf`.
Reference вычисляется независимо eigendecomposition/exponential, не тем же BE
или native assembly. Каждый ненулевой mutual response имеет собственный scale.
P7 предоставляет ещё одну tiny physical matrix + expected response и mapping
для shared import/frequency/time contract test.

Не менее трёх direct JS scenarios: (1) standalone RC/RLC AC+finite-driver/RC
receiver, (2) `STM32G0_CAN_RS485` authored electrical hypothesis on verified
existing connector/component contacts, (3) `ThunderF722` authored line/receiver
and finite reflections, with exact chosen contacts frozen after fixture
inventory. Board IDs — provenance only: эти сценарии не называют заданную сеть
извлечением PCB. Каждый сохраняет exact DSL/resolved models, complete axes/V/I,
complex transfer/phase, mapping, units/validity, refinement/error arrays, hashes
и ресурсы. Один намеренный fail check сопровождается последующим pass.

Отрицательные проверки: missing/duplicate port/source/reference, wrong spectrum
unit/grid, zero transfer denominator, ambiguous multi-source transfer,
missing nominal/parasitics, unsupported PRBS, initial/event impulse,
nonsymmetric/indefinite matrices, floating gauge, singular pencil/instantaneous
feedback, invalid delay, finite-table/DC/band refusal, active time without stable
supported realization, history/report/factor budgets, cancellation/timeout/recovery.
Saved-report audit получает mutations axes/units/IDs/V/I/port ordering/missing
mutual/model provenance/refinement/quality; missing/conditional не заменяется
нулём/valid. PNG строятся из тех же numerical arrays и осматриваются; browser
HTML controls остаются непроверенными.

### 6.6. Решения, ожидающие checkpoint архитектора

1. Разрешить существующие `.sources/.loads` на circuit с указанной семантикой
   Spectrum; синтаксис новых AC полей не нужен.
2. Разрешить existing ComponentDescription в `CircuitElement.kind:'model'`
   для nominal resolution, либо явно сохранить этот подпункт открытым.
3. Принять минимальные analytical resource formats `coupled-rlc` и
   `delayed-scattering`, narrow existing `.si().network` execution и честный
   finite-table/time отказ. Это не новый SI/PI analyzer и не P8/P11.
4. Утвердить general-incidence DTO после concrete P7 coupon; common electrical
   code/types принадлежат P6, extraction/recovery — P7.

После checkpoint — E1/E2 small tests, E3/E4 implementation, focused gates и
исправления; для native/package/real fixtures запрашивается общий тяжёлый слот,
`CARGO_BUILD_JOBS=1`, отдельный target, обычный placement GPU host сохраняется.

### 6.7. Поправка checkpoint: sampled models имеют положительный путь E3

Решение архитектора: E1/E2 разрешены, ComponentDescription в model element
разрешён; descriptor math принят до concrete P7 coupon. Exact delay — полезный
дополнительный distributed subset, но **не закрывает** time response sampled
models. Поэтому blanket refusal finite tables из первоначального §6.4 не является
итоговым решением E3. Основной sampled путь предлагается следующим.

Поддержанный класс — reciprocal passive relaxation admittance с известным DC,
известными high-frequency conductance/capacitance и постоянным real orthogonal
modal basis U (первый обязательный класс U=I, 1..4 порта; symmetric two-port
even/odd U — второй). После существующего S/Z/Y→Y преобразования:

```
Y_fit(s) = U diag(y_m(s)) U^T
y_m(s) = g0_m + s*cInf_m + sum_k h_mk * s/(s+p_k)
p_k > 0,  h_mk >= 0,  g0_m >= 0, cInf_m >= 0
sum_k h_mk = gInf_m - g0_m >= 0
```

`gInf` однозначно означает `lim_{s→+infinity}(y(s)-s*cInf)`, то есть
conducting remainder после выделения `s*CInf`, а не сам растущий admittance.

Каждый summand реализуется положительной series-RC ветвью
`R=1/h`, `C=h/p`; это точная passive realization выбранного rational fit.
Прямой G0 и C∞ — parallel shunts. Re(s)>0 даёт
`Re[s/(s+p)]>0`, поэтому вся модель positive real **во всей** right half-plane,
а не только в проверенных samples. Poles строго слева; realness/reciprocity и
causality следуют из формы. Постоянный U с одинаковым преобразованием V/I
сохраняет мгновенную мощность. Никакого clipping отрицательных residues после
unconstrained fit. Noncommuting modal matrices, resonance/complex-pole class,
nonreciprocal/active resource, delayed/noncausal table, неизвестный DC/асимптотика
в этот fit не входят и получают адресный отказ.

Это ограниченная structural fitting процедура, не заявка на полноценный VF.
Перечитанный [Gustavsen–Semlyen VF paper, pp.1052–1053](https://www.sintef.no/globalassets/project/vectfit/vf_paper.pdf)
обосновывает rational recursive convolution и отдельно отмечает known constant/
proportional terms; [их passivity paper](https://www.sintef.no/globalassets/project/vectfit/references/passivity.pdf)
не позволяет ограничиться stable poles. Собственный sufficient PR proof выше
устраняет необходимость общего passivity-enforcement QP для данного класса.
Дополнительный первичный пример nonnegative real-pole synthesis:
[Mendoza-López et al., IPST 2025](https://www.ipstconf.org/papers/Proc_IPST2025/25IPST035.pdf).
Код из статей не копируется; лицензии/reference source фиксируются в verification.

Минимальное дополнение existing frequency Resource.data, до реализации на ревью:
optional `temporal` metadata у `network-parameters`/`touchstone`:
`{method:'passive-relaxation', modeTransform?:number[][], normalizationOhm?:number|Record<string,number>,
highFrequency:{conductanceS:number[],capacitanceF:number[]}}`.
Это явное разрешение построить указанную **conditional** модель по samples,
не универсальная система boundaries и не гарантия неизвестного поведения вне
исходной полосы. Требуется реальный finite DC sample f=0; отдельный выдуманный
low-frequency anchor не добавляется. Оригинальные bytes/hash/samples/refs не
меняются; fit и его coefficients/error/closure assumptions публикуются отдельно.

Первое fixed admission: <=4 ports, <=4096 frequencies, 8/16/32/64 real poles,
<=256 relaxation states, existing request memory/time/report limits. Poles —
log grid в интервале `[10*2π*f_firstPositive, 2π*f_max/10]`; пустой интервал
отклоняется. Это требует наблюдаемой полосы с запасом по обе стороны всех
fitted relaxation scales. DC фиксирует G0; c∞ и g∞ заданы metadata. Fit весов —
bounded nonnegative constrained least squares на simplex с точной суммой
`g∞-g0`; endpoints не определяются неявной extrapolation. Fixed iteration budget
и проверка оптимальности/conditioning; неуспех — failed fit, не нулевые residues.

Interlaced fit/holdout rows выбираются до fitting; после fit проверяются **все**
исходные complex coefficients. Нормировка — dimensionless power-wave scale
`diag(sqrt(Rref))*ΔY*diag(sqrt(Rref))`, исходные Y residuals тоже публикуются.
Для S берутся исходные real references, для Y/Z требуется explicit temporal
`normalizationOhm`; скрытой нормы 50 Ohm нет, эта норма не создаёт физическую нагрузку.
Оба fit/holdout max errors <=0.005 +1e-9 absolute на normalized coefficients;
все существенные off-diagonal/modal responses имеют отдельные relative/errors.
U orthogonality <=1e-10, transformed off-diagonal samples <=1e-9 normalized;
иначе выбранный class неприменим. Final model-order last-pair response change
<=0.001; если 64 poles не достигают условий, отказ без повышения order cap.
Допуски не заменяют continuous band proof: between-sample uncertainty остаётся
conditional; слишком редкие samples/дырки в полосе (>0.1 decade у fit region)
отклоняются. Missing/noisy DC либо несовместимая asymptote не исправляются fit.

Время считает **полученную** causal passive realization в existing MNA с явными
driver/load и steady initial states всех fitted RC. Модельная ошибка — отдельно
от BE temporal error. Положительные references до имплементации: (a) imported
nonconstant 1-port Y of G0 || C∞ || two series-RC branches, (b) supplied two-port
Touchstone S, полученный независимой complex формулой из fixed even/odd RC modes,
с разными relaxation times и nonzero coupling. Для обоих: реальные finite DC
и 8-decade samples + authored high-frequency limits, step/finite ramp с известной
independent exact RLC/matrix-exponential reference. Comparison <=0.1% swing
+1e-6 V, temporal last-pair <=0.05%, independent model-order <=0.1%; если model
fit 0.5% недостаточен для этих time references, tightening fit/model error обязателен,
а временной tolerance не повышается.

Достаточность полосы для сценария: fastest declared source front >=10/f_max,
slowest fitted pole >=10*2π*f_firstPositive, >=20 report samples на fastest front
для peak/arrival assertions; source события всё равно обрабатываются internally.
Source corners не объявляются строго bandlimited: metadata задаёт явную
relaxation-class/high-frequency closure, результат всегда conditional. Это
численный отклик указанного приближения, **не гарантия** неизвестного original
response вне band. Более быстрый stimulus, неизвестное начальное состояние,
incompatible connection/gauge или неоднозначный imported return дают отказ.
Weak-loss P7b table без подтверждённого DC/relaxation closure этим путём не проходит.

Негативные fit references: отрезанный DC, неправильная asymptote, resonant RLC
outside class, time advance/negative delay samples, active gain, нехватка полосы,
wrong units/ref topology, переставленные/missing rows, fit/holdout corruption,
nonconstant modal transform, order cap, необъявленные initial states. Явный
noncausal пример должен не пройти accepted fit gate; успешная PR approximation
конечного набора сама по себе не доказывает causality неизвестного исходного
объекта, что остаётся указанным ограничением conditional model.

Дополнительно: одинаковая physical Y при разных positive real port Zref
должна дать ту же fit/time realization; сравнение normalized power waves и
orthonormal even/odd V/I transform проверяется независимым power identity.
Hidden narrow-resonance oracle строится между одинаковыми sampling nodes:
regular grid не доказывает отсутствия intersample peak. Если samples не
различают этот объект и relaxation model, результат остаётся conditional со
словами «между samples не доказано», во всех derived values/checks/reports;
такая approximation не получает valid только из-за малого holdout residual.
Samples, которые разрешают resonance, должны явно отвергнуть relaxation fit.

### 6.8. Infinity-C refinement обмена с P7

Concrete P7 coupons прочитаны в `next-task-pcb-electrical-extraction.md §7.3`
его worktree. Для concentric shells независимо выводится
`Zdiff=1/(jω)*[[1/c1+1/c2,1/c2],[1/c2,1/c2]]`. Нулевой общий charge даёт
v_outer=0 как следствие, но при Q0!=0 сохраняются v_outer=Q0/c3 и дополнительная
энергия Q0²/(2c3). Поэтому первоначальное описание «nonreference node IDs» в
§6.3 относится только к grounded модели и недостаточно для infinity-C.

Предложение final payload для дальнейшего checkpoint:

```
format: 'coupled-rlc'
referenceTopology: 'grounded' | 'infinity'
potentialNodes: string[]          // infinity: ВСЕ metal potentials
currentDofs: string[]
incidence: [row, column, value][]  // A; dimensionless
portIncidence: [row, column, value][] // B; ordered Resource.ports
resistanceOhm / inductanceH / capacitanceF: [row,column,value][]
conservedPotentialModes?: number[][] // candidate columns N, проверяемые solver
```

Matrices имеют заданные dimensions из ID arrays, full symmetric entries,
no duplicates и finite coefficients. Grounded режим имеет явный mathematical
reference уже исключённый при построении; infinity режим ничего не выбрасывает.
Для infinity columns A/B conserved в указанном полном basis. R при nI>0 в
первом supported subset положительно определена (finite conductivity P7);
L/C допускают PSD и нулевые blocks, pure C допускает nI=0. Scope/band/source/
recovery остаются обычными Resource validity/provenance и P7 report.

Проверяемые N должны удовлетворять A^T N=0; R SPD делает их кандидатами DC
nullspace potential modes. При isolated current-excited DC steady требуется
`N^T B ip=0`. Initial invariants — `q0=N^T C v0`; steady representatives выбираются
по `N^T C v=q0`, а не grounding metal. Initial neutral означает только явно
заданные сохраняемые invariants=0. В time всегда проверяется
`d(N^T C v)/dt=N^T B ip`; ток между conductive islands может переносить заряд
между ними. Нейтральность каждой island автоматически не накладывается.

N применяется к **полной** системе после source/load/port coupling: их
constraints могут уменьшить nullspace. Подмодель не навязывает собственные
сохраняемые заряды независимо от внешних электрических путей. Неполный basis,
неразрешённая gauge, несовместимый DC ток и unspecified required initial charge
должны дать адресный отказ; исходная KCL и charge проверяются после solve
независимо от gauge equations. Singular C на null modes требует отдельного
explicit mathematical gauge; нулевой diagonal не превращается в шунт.

Обязательны sphere AC neutral/charged, pure-C nonzero DC rejection, сохранение
общего charge при balanced transient и transfer charge между отдельными
conductive components, wrong-metal-ground negative, basis scaling/permutation
для P7 RLC coupon. Реализация и focused references находятся в
`native/analyze/src/electrical/coupled/` и `tests/coupled.rs`; статус numerical
acceptance фиксируется только после actual native run.

### 6.9. Implementation refinement: bounded pole dictionary и time boundary

Прототип fixed dense grids показал плохо разрешённый simplex KKT при близких
полюсах. Поэтому starting dictionary остаётся 8 logarithmic poles в том же
авторском band bracket; после каждого constrained fit добавляются 4 geometric
midpoints интервалов с максимальным `(h_i+h_{i+1})*log(p_{i+1}/p_i)^2`.
Tie-break — ширина log interval. Максимум 64, все p положительны; changing pole
dictionary не меняет physical model class. Проверки train/holdout/order остаются
на 8/16/32/64 с прежними gates. Holdout rows не выбирают добавляемые poles:
weights и enrichment используют только заранее выделенные training rows.
Активный simplex NNLS проверяет неотрицательность, сумму и KKT <=1e-9;
singular/ill-conditioned constrained system отклоняется без regularization.

`normalizationOhm` допускается также для S, чтобы сравнение идентичных physical
Y после разного port renormalization использовало единую явно заданную норму.
Без override у S остаются authored source references; Z/Y override обязателен.
S conversion всегда использует его исходные power-wave references, норма
error gate не вмешивается в физическое преобразование.

Первый time boundary — `a.si().network(singleExplicitNetwork)`; sampled
relaxation и полный coupled RLC входят в один native descriptor path.
Driver — finite `linear-driver`, receiver — explicit `rc-input`. Existing bits,
PRBS и absolute Waveform адаптируются к finite-front PWL/формулам; PRBS orders
7/9/15/23/31, MSB before shift, no inversion, feedback MSB XOR bit(tap-1),
taps6/5/14/18/28. Полиномы подтверждены первичными manufacturer references:
[TI SPRUHO3B table19-1](https://e2e.ti.com/cfs-file/__key/communityserver-discussions-components-files/791/spruho3b_5F00_serdes.pdf),
[AMD AM017 TX patterns](https://docs.amd.com/r/en-US/am017-versal-gtm-transceivers/TX-Pattern-Generator).
Это соглашение воспроизводимости stimulus, не совместимость фазы/инверсии
со всяким аппаратным генератором. Explicit bits <=20000, finite rise/fall<=UI.

Generic descriptor initialization сохраняет положительные E charge/flux
eigenspaces и удовлетворяет algebraic equations в null(E). Во время решения
charge не замораживается. Изолированный mathematical zero-energy gauge имеет
отдельную constraint, infinity potential не выбрасывается. BE increment equations
проверяются после solve без gauge multipliers. Energy audit включает source
work, branch R, finite driver Rout, receiver R и BE numerical dissipation.
Continuation identity включает matrices, port mapping и assembled loads/drivers.
Discontinuous fronts пока явно отклоняются до корректной impulse/projected-event
поддержки. Initial steady не означает zero capacitor charge при nonzero DC rail.

Public `a.connect` time composition, explicit distributed delay resources,
independent global time refinement/model-order references и direct native DSL
acceptance ещё не считаются выполненными из наличия этого boundary. Frequency
P6b без temporal metadata остаётся отдельным неизменённым методом. Полное P6
может быть закрыто только итоговой матрицей; resonant sampled fitting и P7b
lossy tables-to-time всё ещё вне выбранного accepted class.
