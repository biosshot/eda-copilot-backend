# P9: переходные расчёты на проверенных сетках малых плат

2026-10-24. Band boundary checkpoint прошёл; steady control0.541754%,missing0.
TPS steady control0.8812% принят ранее. A/B single-slot медленнее55.2%,
production cache НЕ менять. Whole P9 ещё не принят.

## Разрешённая работа

1. Два прямых DSL сценария TPS/Band: прежние источники, cooling/ambient history
   и initial state, но spatial refinement policy, подтвердившая steady.
   Transient должен сам подтвердить spatial accuracy на всех output/event times;
   steady convergence не подставлять вместо этого доказательства.
2. Для каждого — обычная и ужесточённая временная точность на одной физической
   постановке. Сравнивать поля в общей области, witnesses/coverage, original
   nodal/energy, temporal<=0.1%, spatial independent control<=2%. Output step
   не подменяет internal timestep. Preserve original events/sampled extrema.
3. До каждого запуска admission geometry/memory/output; explicit study deadline
   разрешён600s на один полный adaptive request,384MiB/50k nodes/128MiB и native
   numeric output caps прежние. Это увеличение только явно заданного времени
   проверки относительно120s; production defaults не менять. Общий deadline,
   не свежий на каждый pass. Всего<=4 public requests, без повторных серий tuning.
4. При отказе/недостатке бюджета сохранить stage costs/actual witnesses, не
   повышать ресурсные caps молча и не скрывать failed global comparison.
5. Добавить план корректного curve-tolerance сравнения по сохранённой geometry:
   общая физическая область и симметрическая разность, source identities,
   boundary displacement/area и отдельные coverage diagnostics. Не выбрасывать
   изменившуюся область и затем не объявлять сравнение полным. Пока без нового
   большого controller.01 run.

## Границы

Controller full7pass transient пока не запускать. По новым phase costs малых
плат подготовить одну конкретную оценку controller time/memory/output и
необходимого ресурса/метода, без экстраполяции coupon speedup как доказательства.
Никаких новых cache/API/CFD/моделей. Не повторять green steady/controller,
addon/native tests без changes. Использовать текущий штатный addon.
Новые direct reports/PNG/manifest и audit negatives обязательны; сохранить все
исторические failures. После пакета остановиться, коммитов/новых агентов нет.
