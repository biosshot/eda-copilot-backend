# P7: минимальный native port проверенной layered C постановки — proposal

2026-10-24. Только предложение для следующего review; реализация не начата.
Основание: [§8.15](../tasks/next-task-pcb-electrical-extraction.md#815-final-d64-domainmesh-controls-2026-10-24),
`debugging/p7-design-checkpoint/domain64-capacitance-audit.json`.
Finite vacuum plate coupon: independent full/common4.41974/4.41954%,
differential2.89846%; separate domain.78250%/mesh.60540% full changes.
Это достаточное основание для ограниченного native coupon port, не production
acceptance P7 и не доказательство arbitrary PCB.

## Первый законченный implementation блок

1. Перенести только проверенный CPU/F64 SI electrostatic product space
   P1(XY)*P1(z): degree2 XY moments на exact triangle overlaps, analytic z,
   finite copper volume exclusion и полный metal-face/sidewall trace. Текущий
   tetrahedral `local::CapacitanceOperator` не объявлять проверенным этим coupon:
   interpolation space и allocation representation должны соответствовать
   принятой постановке. Работать внутри существующего extraction/local модуля,
   без нового общего solver/service и без дублирования P6 time/network solver.
2. Переиспользовать pinned Spade CDT из `conduction/mesh.rs` и существующие
   geometry/source-ID contracts. Общий endpoint/constraint registry должен
   сохранять исходные polygons, holes, foil faces, layer spans и source ownership.
   Не наследовать terminal-alignment/collinear-drop policy без отдельного
   exact-geometry доказательства. Для независимых XY planes — conservative
   common-refinement trace; material classification по provenance, не midpoint
   snapping. Существующие local mesh validators переиспользовать где совместимы.
3. Переиспользовать pinned faer LLT, actual symbolic planning и bounded native
   scratch. Preallocated typed COO/CSC вместо retained boxed scalar triplets;
   до allocation проверять term/nnz/port-RHS counts и simultaneous peak, включая
   geometry/assembly/values/factor/scratch/output. Старые production API/default
   caps не поднимать автоматически до diagnostic50k/6M. При отказе вернуть
   явное unavailable/resource diagnostic, не clipped geometry/zero coefficient.
4. Сначала internal native coupon adapter, затем только после review boundary
   к существующему analysis extraction DTO. Полная C матрица сохраняет все
   conductor potentials; explicit infinity truncation не превращает conductor
   в hidden ground. Charge, field recovery и W=.5v^TCv должны использовать тот
   же trace. Source/contact mapping к P6 согласовать с его владельцем; не
   вводить новый public DSL или обещать ready RLC/channel на основании C.

## Обязательная приёмка перед real-board admission

- Продвинуть минимальные reproducible coupon geometry/mesh manifests, failed
  D32 и accepted D64 controls из ignored diagnostics в tracked native fixtures;
  frozen FastCap raw unsymmetrized reference/input hashes сохранить. Для exact
  same mesh native/Python C parity target1e-10 relative quadratic energy, затем
  independent full-C5% и отдельные XY/z/domain changes<=2%; parity не заменяет
  независимый accuracy gate. Новую CDT triangulation оценивать заново against
  external reference, а не ожидать совпадения другой сетки.
- Добавить independent full-matrix accuracy coupons: aligned multiple dielectric
  layers с двумя eps; shifted finite foils на разных layers; три conductors с
  floating/common modes и explicit return. Проверять full positive-range energy,
  scalar raw entries, conservation/reciprocity/work и recovery, не только C11.
- До поддержки vias: finite plated barrel/span/bore и dielectric interfaces,
  затем пара vias с mutual C/return; источник геометрии и трёхмерный trace
  независимы от mesh IDs. До прохождения таких coupons vias остаются unsupported.
- Negative/resource coupons: отсутствующий материал/far trace, overlap/ambiguous
  ownership, source loss/unsupported topology, nonfinite coefficients,
  rank/conditioning и allocation refusal. Искусственно relaxed tolerances и
  hidden grounds не допускаются.
- После этих gates: bounded ROI на трёх существующих исходных blueprints с
  сохранёнными ports/source IDs, без изменения CAD/export. Public integration
  и выдачу модели отдельно review; геометрия/порты не подменяются net names.

## Подтверждённые пределы и открытые обязательства

Измерены только два finite1mm-square35um vacuum plates с965um gap. D64 final
593XYnodes/plane x69planes=40917nodes,2.79648Mlocal terms; faer factor228.06MB,
whole solve10.16s и sampled combined375.23MB при512MiB/60s. Domain/final control
caps50k/6M — только diagnostic authorization, не реальные arbitrary-board limits.
В native budget/real ROI cost должны учитываться actual CDT intersections,
symbolic fill и output, поэтому эти числа не обещают пригодность всех трёх плат.

External full margin до5% только~.58percentage point; это не asymptotic error
bound. Layered dielectric/via/несколько conductors пока не валидированы этим
coupon. Dispersive/anisotropic dielectric, losses/frequency bandwidth и general
3D geometry не принимаются автоматически. L kernel conditioning/cancellation,
actual ROI cost и full magnetic quadratic gate остаются отдельными blockers;
нельзя заменить их C или формулой локальной линии.

Следующий шаг — решение review о таком ограниченном native coupon port.
Никакой production код, новый solve или commit этим proposal не выполнен.
