# Thermal: один extended TPS transient для проверки временной сходимости

Пользователь явно разрешил один более долгий расчёт. Исполнитель thermal_medium, worktree analyze-thermal. Это отдельная диагностическая проверка точности, не изменение production SLA/defaults и не одобрение rejected supernodal path.

## Ровно один численный запрос

Плата TPS63060DSCR. Повторить прежний thermal-final-time-refined.js, изменив ТОЛЬКО explicit maxSeconds с600 на1800 (30мин). MaxMemoryMiB384, maxNodes50000,2M numeric numbers,128MiB result, geometry/materials/sources/cooling, mesh refinement policy и весь60s physical history остаются прежними. Строгие temporal tolerances2.5e-5/2.5e-6, исходные residual/energy/spatial gates не менять.

Создать отдельный прямо читаемый DSL-файл thermal-extended-time-refined.js и тонкий launcher по прежнему образцу. Не редактировать frozen старый сценарий/expectations/failed evidence. Launcher timeout2100s — только запас на транспорт/сохранение ошибки; физический shared deadline1800s. Не добавлять повторный .run() в сценарий и не выполнять обычный расчёт повторно.

## До запуска

Сверить исходную плату, authored physics, schema/numerics, baseline field hashes и provenance сохранённого ordinary TPS (final-time: spatial1.546716%,missing0). Предпочесть исходный проверенный addon6b86525c1372b1472a2e4643b97c16f1f75db1dc192d031387e8fdf4135447dc из сохранённого snapshot, чтобы оставить идентичный solver. Не перезаписывать main addon. Допустим временный локальный launcher/изолированный package с этим binary.

Если нельзя воспроизвести совместимый runtime или доказать неизменность физических алгоритмов/обычного результата, НЕ тратить один запуск: выдать blocker. Более новый normal addon допускается лишь с уже сохранённым доказательством эквивалентности затронутых production paths; test-only prototype не включать. Никаких новых algorithm/cache changes в этом этапе.

До запуска проверить locks/пакет/доступность inputs и outputs без расчёта, документировать preflight max memory/output и причину выбора1800s. Это разрешённый диагностический ceiling, не гарантированное время завершения. Thermal получает первый эксклюзивный heavy slot, P7 пока делает чтение/планирование.

## После единственного запроса

При успехе сравнить с сохранённым ordinary по полным сопоставимым полям и тем же временным наблюдениям, layers/source/contact coverage. Прежний accepted comparison helper и ambient-rise normalization, temporal <=0.1%, spatial <=2%, missing0; отдельно абсолютные/относительные witnesses и все original nodal/energy guards. Одна junction-temperature не заменяет full-field comparison. Если geometry/refinement meshes различаются, использовать проверенный physical-domain interpolation/coverage, а не сравнение по индексам.

Сохранить wall/RAM, stages, accepted/rejected steps/iterations если доступны без изменения solver, полные массивы и numeric overlays/temperature-time difference maps. Запустить saved negative substitutions и визуально проверить PNG; plotting/audit не запускают solver повторно.

При deadline/memory/output/physical failure сохранить точный отказ и диагностику; не увеличивать лимит, не повторять, не уменьшать физический интервал/поле, не выдавать частичный результат за сходимость. Не запускать Band/controller/curve или ordinary. Успех означает временную проверку ЭТОГО сценария, не весь P9 и не соблюдение600s performance target.

После работы освободить heavy slot P7 и STOP. Без коммита/новых агентов. Результат: thermal-extended-transient-verification.md и evidence manifest, все числа/ограничения/совместимость baseline явно указаны.
