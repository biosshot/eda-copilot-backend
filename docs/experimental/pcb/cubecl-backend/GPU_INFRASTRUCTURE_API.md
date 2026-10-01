# Внутренний GPU API

`native/pcb-board-packer/src/compute/` входит в существующий crate и addon. Все его GPU модули и типы закрыты `feature = "gpu"`; CPU-only сборка не включает CubeCL/wgpu. Зависимость направлена от домена к `compute`; инфраструктура не импортирует solver.

## Runtime и доступ

`gpu::with_session(requirements, operation)` лениво получает единственную process-wide сессию. `Requirements` задаёт требования потребителя, `Capabilities` содержит проверенные возможности client. Текущая политика инициализации сохраняет выбор Vulkan F64 адаптера и проверку F64/U64 с точным arithmetic probe. Возможность выбирать другой тип адаптера или precision в этой миграции не добавлена.

Mutex защищает всю связанную GPU операцию: upload, dispatch цепочки kernels и необходимый readback. Генерация кандидатов и доменные проверки входа выполняются снаружи. Нельзя захватывать эту блокировку рекурсивно, включая вызов `statistics()` из operation. Между процессами действует прежняя OS lease; занятый lease переводит runtime в `Busy` с повторной проверкой через одну секунду. `ready()` проверяет состояние без инициализации и без повторного получения занятого lease.

`Error.kind` доступен без разбора сообщения: `Busy`, `Disabled`, `NoDevice`, `MissingCapabilities`, `Lease`, `AdapterMismatch`, `ArithmeticIncompatibility`, `RuntimeFailure`, `InvalidInput`. Runtime не вызывает CPU solver. При ошибке инициализации или runtime failure сессия становится `Disabled`; повторная инициализация не выполняется. Panic перехватывается внутри mutex. Домен отбрасывает свой Engine и самостоятельно выбирает границу CPU replay. Block controller сохраняет полный повтор исходного native call; deferred pairs повторяются с исходным `pairSeed`.

`InvalidInput` и отсутствие запрошенной потребителем capability у уже готовой сессии не отключают runtime. `InvalidInput` разрешён только до GPU работы; после dispatch операция обязана завершить необходимый readback либо вернуть `RuntimeFailure`. Доменная неподдерживаемая геометрия и небезопасные численные значения проверяются block solver до передачи их GPU и не становятся ошибкой общей сессии.

## Память

`Session::workspace(ScratchKey::new(layout, local_slot), bytes)` возвращает handle из одного общего workspace. Уникальное статическое имя layout назначает потребитель; local slot имеет смысл только внутри него. Block scorer хранит значения в своём `ScoreScratch`, frontier — в `FrontierScratch`. Инфраструктура не знает их форматов. Рост сохраняет прежнюю политику: минимум 8 байт и округление capacity до следующей степени двойки; меньшие последующие запросы переиспользуют capacity. Отдельных contexts или заранее выделенного пула нет.

Scratch принадлежит защищённой операции. Его handles можно клонировать для kernels этой операции, но нельзя сохранять в Engine, возвращать для последующего GPU использования или читать после передачи workspace следующему заданию. Rust `Handle` сам по себе не кодирует эту границу времени; соблюдение проверяется в местах unsafe запуска kernels. Операция возвращает CPU данные после readback.

Resident templates, frames и кеши создаются через тот же session client, принадлежат доменному Engine и могут переживать отдельные операции. Домен управляет их содержимым и инвалидацией. При runtime failure Engine уничтожается; его handles нельзя использовать с другим runtime. Автоматическое восстановление устройства и эпохи кешей не введены.

При отключении сессии сбрасываются scratch handles и OS lease. Глобальный CubeCL client/allocator может удерживать context и память драйвера до выхода процесса. `workspaceBytes` считает только capacities scratch, включая все уже использованные layouts; это не суммарная VRAM, resident память или память allocator.

## Числа и диагностика

`compute::numerics` содержит перенесённые без изменения операций `rp`, `grid_quotient`, `rounding_probe` и их прежние граничные тесты. CPU `geometry::round_placement` остаётся эталоном. Block scoring, MST, collisions, pruning и ranking остались в домене; FMA/fast-math политика и F64 точность не менялись.

`statistics()` сохраняет `initializations`, накопленный `mutexWaitMs`, `workspaceBytes` и добавляет device, capabilities, state и типизированный `unavailableReason`. Подробные доменные счётчики и стадии остаются в block solver. Подробное профилирование не включается автоматически.

Сохраняются прежние `PCB_BLOCK_BACKEND`, `PCB_BLOCK_GPU_DISABLED`, `PCB_BLOCK_SOLVER_PROFILE`, validation и fault injection flags, лог-теги и `placement-bench` NAPI probe. Новых aliases переменных среды нет; приоритет прежних flags не изменён. Сохранено поведение: только значение `PCB_BLOCK_GPU_DISABLED=1` запрещает инициализацию, а наличие `PCB_BLOCK_SOLVER_PROFILE` включает соответствующие логи.
