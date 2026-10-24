# P6: смешанные временные сети delay + RLC

2026-10-24. Продолжение принятого `61366df`, не начало PCB SI/P11.
Исполнитель — прежний P6 worktree; архитектор проверяет постановку и результат.

## Зачем

Сейчас отдельно работают точная задержка, физическая RLC-сеть и разрешённая
sampled-RC relaxation. Нельзя соединить их через существующий `a.connect`
и получить один временной отклик. Следующий блок закрывает этот конкретный
стык: линия с задержкой + дискретный фильтр/нагрузка, несколько отражений,
влияние R/L/C между участками линии. Это авторские электрические модели;
извлечение полной трассы, SI channels, TDR, eye и timing остаются P7/P11.

## Границы

1. Сохранить существующие `a.model`, `a.network`, `a.connect`,
   `a.si().network(...).drivers(...).receivers(...).run()` и именованные порты.
   Не добавлять второй builder, пользовательский solver selector или DSL.
2. Поддержать смешанный граф уже разрешённых real-K delayed-scattering,
   grounded canonical coupled-RLC и passive-relaxation моделей. Начать с
   того же явного common return и прежних парных connections. Не превращать
   infinity reference в землю, не придумывать новые физические контакты.
3. General incidence RLC остаётся доступен отдельным блоком. Произвольные
   infinity/noncanonical joins — отдельный оставшийся стык P6/P7: в этом
   задании дать адресный отказ и записать необходимые данные, не реализовывать
   скрытое grounding. General resonant/active fitting также вне этого блока.
4. Сохранить точную задержку, не заменять её Padé или RC fitting. Сохранить
   текущие ограничения пассивности, конечных фронтов, полосы и состояний.
   Частотные таблицы без temporal closure не приобретают её от соединения.
5. Rust CPU/F64, прежний shared addon и placement F32/GPU. Никаких CAD,
   RoutingBoardV2, Thermal, геометрии, compute-runtime или GPU изменений.

## Контрольная точка до production-кода

В этот файл дописать короткую схему сборки: неизвестные, знак токов/волн,
уравнения zero-delay и positive-delay блоков, внутренние соединения,
steady prehistory, descriptor constraints и первоначальные заряды/потоки.
Показать, как существующие coupled/time и delayed переиспользуются без двух
различающихся реализаций источников, нагрузки и энергетического баланса.
Указать нужные изменения TS/native DTO, caps и владельцев файлов.
Архитектор проверяет checkpoint; пользователь повторно ничего не утверждает.

Математическая основа — общий descriptor и история принятых шагов. На текущем
шаге положительная задержка даёт известную историю; нулевая задержка входит
в текущую алгебраическую систему. Состояния RLC и RC receiver решаются совместно.
Идеальная сквозная связь при нулевой задержке не должна требовать обращения
сингулярной Y-матрицы. Проверять исходные уравнения, а не только их factorization.

## Численные требования

### Design checkpoint — 2026-10-24 (до implementation)

**Architect review: одобрен 2026-10-24.** Проверены знаки wave/KCL,
аналитические RC/RLC denominators и отражения; можно реализовывать в заданных
границах. Приёмка implementation остаётся отдельной. `Dbe` — диссипация метода,
а не физическая потеря компонента; в отчёте показывать также balance без её
вычитания и её уменьшение при уточнении. Signed history defect не исправляет
ни один acceptance gate. Не вводить механизм частичной приёмки failed traces.

- **Единая сборка.** `coupled/time.rs::solve` направляет любой граф с delay в
  общий backend `electrical/delayed.rs::time`; прежний all-delay использует его же.
  Pure coupled сохраняет general-incidence путь. Неизвестные `x=(v,j,a)`:
  merged positive node potentials, RLC branch currents, incident power waves.
  Coupled/relaxation blocks проходят существующие `composition::resource` и
  `System::compile` проверки до суммирования; canonical B связывает порт с одним
  физическим positive node, return общий. Парные connections объединяют nodes;
  внутренние port currents входят в один KCL и не становятся нагрузками.
- **Волны и descriptor.** Ток `i` направлен внутрь delay блока;
  `v=sqrt(R)(a+b)`, `i=(a-b)/sqrt(R)`, `b=K a(t-d)`.
  При `d=0`: строка `-v/sqrt(R)+(I+K)a=0`, KCL добавляет
  `(I-K)a/sqrt(R)`. При `d>0` известный `b` идёт в RHS:
  `-v/sqrt(R)+a=-b`, KCL содержит `a/sqrt(R)` и RHS `+b/sqrt(R)`.
  Полная система `E dx/dt+G x=f(t,history)` включает C/L и receiver C;
  волновые строки имеют нулевую E. Y не обращается, идеальный zero-delay thru
  остаётся алгебраическим ограничением. Проверяется исходный residual/KCL,
  включая delay rows, после решения и восстановления gauges.
- **Общие операции.** Вынести только необходимые shared source/load admission,
  stamps и endpoint power/BE-storage helpers из существующих time путей;
  all-delay и mixed не имеют разных источников, receiver C или energy ledger.
  Использовать существующие sparse solve/BE increment и step-doubling helpers;
  wave rows не участвуют в положительной storage quadratic form.
- **Initial/history.** Steady matrix G0 включает K всех delay blocks, конечные
  Rout/loads и source(t0). Решить полную сеть один раз; постоянная prehistory
  incident waves равна этому решению. Начальная энергия включает C/L/receiver C
  и delay integral, в том числе ненулевой rail. Допускается только steady initial;
  arbitrary charge/flux, incoming-wave history и continuation для mixed адресно
  отклоняются. Charge/gauge modes определяются после полной zero-delay сборки:
  физические E modes не обнуляются; только доказанный zero-energy gauge может
  быть фиксирован. Неразрешимый/неединственный физический steady state даёт отказ,
  без gmin, hidden ground и per-block neutrality.
- **История и шаг.** Хранить только accepted half-steps, piecewise-linear `a`;
  delayed queries причинны, `h<=minPositiveDelay/4`. Доставлять corners источника
  и их повторные задержанные приходы. Сохранить caps 100000 delivered corners,
  2000000 history numeric values; output cadence не выбирает интеграционный шаг.
  Delay storage `sum integral[t-d,t] ||a(u)||²du`; attenuation loss rate
  `||a(t-d)||²-||K a(t-d)||²`, integral считается точно по linear history.
- **Энергия, заранее фиксированная норма.** Для каждого принятого BE half-step
  endpoint rectangle даёт source work, R/driver/load losses и net wave power.
  Lumped storage change плюс `0.5*delta^T E delta` замыкают BE identity;
  численная диссипация содержит только эту неотрицательную quadratic form.
  Отдельно сохранять signed wave-history quadrature defect (endpoint wave work
  минус exact rolling-history storage/attenuation), никогда не прибавлять его
  к numerical dissipation и не корректировать им acceptance balance.
  Physical balance `Wsource-WR-Watten-Delta(Ulumped+Udelay)-Dbe` проверяется
  независимо: `abs(balance)<=1e-4*N`, где до измерений
  `N=abs(Wsource)+WR+Watten+abs(DeltaUlumped)+abs(DeltaUdelay)+Dbe`.
  При `N=0` требовать абсолютный balance<=1e-20 J. Независимое глобальное
  уточнение history/maxStep подтверждает temporal/energy tolerances задания.
- **Admission/DTO/owners.** Один start/deadline и budget через normalization/fit,
  assembly, initialization, factors, history и output; до allocation учитывать
  одновременно живые source copies, fit workspace, matrices/factors и arrays.
  Defaults 256 MiB/120 s, caps 2048 states/1024 potentials/1536 branches,
  32 blocks/128 internal/16 external ports остаются прежними. Владельцы:
  `native/analyze/src/electrical/{delayed.rs,coupled/time.rs,coupled/composition.rs}`
  и минимальный shared helper при необходимости; native declarations/report
  adaptation лишь для port-current/energy/history diagnostics. TS request DSL,
  shared addon ABI, placement F32/GPU не меняются. В result сохранить все fit,
  conditional/producer-band/permission данные, не расширяя applicability.
- **Независимые эталоны.** (1) Z0=Rs=RL=50 Ohm, delay 2 ns, series Rf=50 Ohm,
  shunt C=30 pF: Hdc=1/3, tau=1 ns, closed-form delayed finite-ramp RC.
  (2) matched lines 2/3 ns, Rf=5 Ohm/L=50 nH/C=20 pF:
  `H(s)=exp(-5ns*s)*50/[105+s*(50nH+55*50*20pF)+s²*50nH*50*20pF]`;
  damped-sine convolution и независимый complex ABCD для всех ports.
  (3) Rs=25/Z0=50/RL=75 Ohm, delay 2 ns, Ctotal=22 pF:
  GammaS=-1/3, GammaL=-1+1.2/(1+s*0.66ns); arrivals 2/6/10/14 ns,
  finite bounce expansion через repeated-pole Erlang/ramp формулы.
  Ни один reference не вызывает production assembly/stepper. Остальные coupons,
  negatives, public reports/audit/install checks — строго по спискам ниже.

- Один deadline и один совокупный memory/output budget на fit, composition,
  initialization, историю, факторизацию и time stepping. Существующие лимиты
  не повышать по умолчанию; оценить одновременно живущие данные до allocation.
- Steady prehistory получается из полной соединённой схемы, в том числе
  ненулевого DC, конечного Rout и нагрузок. Нельзя инициализировать каждый
  блок изолированно или потерять энергию уже заряженного конденсатора.
- Доставлять source corners через фактические задержки. Сохранять прежний
  finite-history/event cap, контроль временного шага и независимость output
  sampleStep от интегрирования. Подтвердить глобальным уточнением истории/шага.
- Общая энергия: работа источников = потери R/driver/load/attenuation + изменение
  C/L/delay storage + отдельно отражённая численная диссипация метода.
  Внутренние connection powers взаимно уничтожаются, не считаются нагрузкой.
- Исходный residual <=1e-8; временной reference error <=0.1% заданного swing
  +1e-6 V; изменение при независимом уточнении <=0.05% swing.
  Нормированный энергетический дисбаланс <=1e-4 для reference coupons.
  Если определение нормы требует уточнения, зафиксировать до измерений;
  не менять допуск после неудачи. Нулевой stimulus не нормировать делением на 0.
- Conditional validity, producer bands и permissions всех блоков сохраняются
  в результате и derived measurements. Не расширять полосу вычисленным ресурсом.

## Независимые тесты

1. Matched delay -> RC low-pass: независимая формула задержанного finite-ramp
   RC отклика; до времени прихода нет искусственного предсигнала.
2. Delay -> RLC -> delay: независимый частотный каскад и time reference
   из аналитического импульсного отклика/свёртки для выбранного согласованного
   случая. Проверять амплитуду, время прихода и затухающий ringing.
3. Неcогласованные finite driver/RC receiver и несколько отражений: независимо
   выписанные первые приходы/коэффициенты и общее уточнение шага. Reference
   не должен вызывать production assembly или тот же интегратор под другим API.
4. Zero-delay идеальный thru + RLC, ненулевой steady initial rail, пассивный
   attenuator с задержкой, два связанных канала с ненулевыми mutual L/C.
5. Та же physical RC через явный coupled ресурс и разрешённый sampled fit:
   эквивалентный смешанный отклик в пределах отдельных model/time ошибок.
6. Перестановки/переименования ports, вложенный connect, разные positive real
   wave references: физические V/I неизменны. Проверить все передачи и отражения,
   а не один удачный S21. Согласовать frequency и time на sinusoidal reference
   после обоснованного переходного интервала.
7. Negative: infinity join, отсутствие temporal closure/DC, active/nonpassive
   блок, неполные/повторные порты, неразрешимый algebraic loop, слишком узкая
   producer band, tiny delay/history budget, общий timeout и отмена/восстановление.
   Не использовать gmin/проектирование PSD/обнуление состояния для green результата.

## Публичные примеры и приёмка

- Прямой standalone DSL и два прямых DSL на существующих STM32G0_CAN_RS485 и
  Thunder boards. Порты/контекст платы реальны; модели авторские, отчёт явно
  сообщает, что это ещё не извлечение трассы. Тонкий runner не строит сценарий.
- Графики V/I, delay/RLC энергии и потерь, независимого reference/error и
  временного уточнения; полный численный result и исходные inputs сохраняются.
  PNG строятся из сохранённых чисел и просматриваются. Не объявлять HTML
  взаимодействие проверенным без фактической проверки.
- Независимый saved-result audit должен отвергать повреждения оси времени,
  амплитуды, energy, validity, ports и неполные cases. Проверить установленный
  пакет/compiled worker со штатным addon, focused native/TS regressions,
  typecheck/build и минимальный cancellation/recovery gate.
- Сохранить manifest исходников/addon/результатов, команды, фактические время
  и память, неудачные измерения. Не повторять неизменённые дорогие тесты.
- Обновить semantics/examples/verification/CHANGELOG и матрицу оставшегося P6.
  Не объявлять весь P6 или P11 закрытым из одного этого блока.
- Heavy слот общий с P7/Thermal, jobs=1, отдельный target. Не коммитить до
  родительского ревью; не трогать чужие worktrees и пользовательские artifacts.
