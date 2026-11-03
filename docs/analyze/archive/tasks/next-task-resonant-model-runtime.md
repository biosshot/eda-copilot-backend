# P6: ограниченная резонансная модель в существующем ядре

Основание: принятый electrical-resonant-checkpoint.md. Исполнитель p6_medium; worktree analyze-p6-completion. Это продолжение P6, не завершение всех SI/PI.

## Что реализовать

- CPU/F64 structural fitting только явно заявленного класса Y(s)=U diag(k*s/(s²+a*s+b)) Uᵀ: 1–2 порта, одна секция на моду, положительные k/a/b, постоянная действительная ортонормальная U. Не пытаться автоматически доказать этот класс по конечным отсчётам.
- Расширить существующее описание временной пригодности frequency Resource отдельным явным discriminator/metadata для этого класса, используя прежние a.model/network/SI builders. Сначала записать точный минимальный объект и единицы в semantics; не добавлять новый пользовательский builder или отдельный solver. Обычные frequency Resources и sampled-RC путь не менять.
- Полная проверка authored U, DC slope/HF closure, конечности, формы, полосы, источника, conditioning, reciprocity и residual во всём matrix space. Не подменять отсутствие данных предположениями, не чинить активность/полюса молча.
- Экспорт в существующий coupled descriptor: A=[U;-I], B=[I;0], положительные branch R/L и internal C. Повторно использовать AC/time execution, сборку, контроль энергии/состояния и бюджеты. Не переносить Python RK4 в production. Проверить, что фактический существующий transient допускает эту структуру: если нет — показать конкретный blocker до изменения общих физических правил.
- У результата и производных сохранять conditional provenance и полосу. Это не универсальный Touchstone-to-time. Parallel-Z с идеальным L, общая VF, шумные произвольные данные, неизвестные дополнительные резонансы и delay inference вне задачи.

## Приёмка

1. Нативные tests на series Y и mutual Y из замороженных fixtures: все matrix entries против независимых физических формул; <=0.5% + прежний абсолютный floor. Closure и преобразование dimensional units проверяются отдельно.
2. Transient через настоящий production integrator против аналитического reference: <=1% заранее заданной шкалы; time refinement <=0.1%; прежние production energy/residual guards без ослабления. Не требовать переноса фиксированного шага 1ns из RK4; шаг выбирает существующий integrator. Сохранить coarse FAIL как историю прототипа.
3. Отрицательные tests: некорректная U/closure/shape/NaN, active samples, плохая обусловленность, недостаточная частотная сетка, полоса источника, неподдержанный Z и неопределённый class. Hidden-resonance fixture остаётся доказательством ограниченности, не обещанием автоматического детектора.
4. Прямой JS DSL, compiled worker и installed package на standalone взаимном двухпорте; AC/transient/energy графики из сохранённых массивов, machine-readable audit и negative substitutions. Можно показать тот же сценарий в контексте реальной платы, но нельзя выдавать authored RLC за извлечённую PCB.
5. Regression physical/coupled/mixed/relaxation и typecheck/build. Один addon с сохранением placement F32/GPU. Frozen reference не менять под ответ реализации.

## Владение и ограничения

Менять только P6 model admission/adapter/electrical modules и их тесты/документацию. P7 и Thermal не трогать. По общим файлам Thermal сейчас занимается лишь native profiling: договориться о слотах сборок. Изолированный target, jobs=1, CPU F64. Численные coupons <=512MiB/60s каждый; установленные runtime limits не повышать. Native build выполнить один раз после focused checks, повторы лишь при реальном исправлении. Без коммита/новых агентов; отчёт с hashes, поддержкой и ограничениями, затем стоп.
