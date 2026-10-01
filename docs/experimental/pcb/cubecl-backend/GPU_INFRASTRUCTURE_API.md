# Внутренний GPU API

`native/pcb-board-packer/src/compute/` входит в существующий crate и addon. Все его GPU модули и типы закрыты `feature = "gpu"`; CPU-only сборка не включает CubeCL/wgpu. Зависимость направлена от домена к `compute`; инфраструктура не импортирует solver.

## Runtime и доступ

`gpu::with_session(requirements, operation)` лениво получает единственную process-wide сессию. `Requirements` задаёт требования F32/опционального U64 потребителя; `Capabilities` содержит проверенные возможности client. Активная [F32 миграция](F32_MIGRATION_ROADMAP.md) больше не требует F64. Проверяются Vulkan Float32 properties, backend фактического client и реальные arithmetic probes. Приёмка всей миграции остаётся открытой; прежние F64 измерения сохранены в исторических results.

Внутренний `PcbRuntime` делегирует выполнение и память одному существующему `WgpuServer`. Обёртка compiled task добавляет в фактически исполняемый SPIR-V RTE, SignedZeroInfNanPreserve и NoContraction и отвергает F64/fast-math/RelaxedPrecision/FMA. Scoped CPU environment и TS helpers используют signed FTZ. На текущем RTX Vulkan оба denorm properties false, поэтому shader канонизирует subnormal inputs/results целочисленными bit operations. Неподдерживаемый execution mode FTZ не запрашивается. Влияние этого pass на полную производительность ещё не принято.

Mutex защищает всю связанную GPU операцию: upload, dispatch цепочки kernels и необходимый readback. Генерация кандидатов и доменные проверки входа выполняются снаружи. Нельзя захватывать эту блокировку рекурсивно, включая вызов `statistics()` из operation. Между процессами действует прежняя OS lease; занятый lease переводит runtime в `Busy` с повторной проверкой через одну секунду. `ready()` проверяет состояние без инициализации и без повторного получения занятого lease.

`Error.kind` доступен без разбора сообщения: `Busy`, `Disabled`, `NoDevice`, `MissingCapabilities`, `Lease`, `AdapterMismatch`, `ArithmeticIncompatibility`, `RuntimeFailure`, `InvalidInput`. Runtime не вызывает CPU solver. При ошибке инициализации или runtime failure сессия становится `Disabled`; повторная инициализация не выполняется. Panic перехватывается внутри mutex. Домен отбрасывает свой Engine и самостоятельно выбирает границу CPU replay. Block controller сохраняет полный повтор исходного native call; deferred pairs повторяются с исходным `pairSeed`.

`InvalidInput` и отсутствие запрошенной потребителем capability у уже готовой сессии не отключают runtime. `InvalidInput` разрешён только до GPU работы; после dispatch операция обязана завершить необходимый readback либо вернуть `RuntimeFailure`. Доменная неподдерживаемая геометрия и небезопасные численные значения проверяются block solver до передачи их GPU и не становятся ошибкой общей сессии.

## Память

`Session::workspace(ScratchKey::new(layout, local_slot), bytes)` возвращает handle из одного общего workspace. Уникальное статическое имя layout назначает потребитель; local slot имеет смысл только внутри него. Block scorer хранит значения в своём `ScoreScratch`, frontier — в `FrontierScratch`. Инфраструктура не знает их форматов. Рост сохраняет прежнюю политику: минимум 8 байт и округление capacity до следующей степени двойки; меньшие последующие запросы переиспользуют capacity. Отдельных contexts или заранее выделенного пула нет.

Scratch принадлежит защищённой операции. Его handles можно клонировать для kernels этой операции, но нельзя сохранять в Engine, возвращать для последующего GPU использования или читать после передачи workspace следующему заданию. Rust `Handle` сам по себе не кодирует эту границу времени; соблюдение проверяется в местах unsafe запуска kernels. Операция возвращает CPU данные после readback.

Resident templates, frames и кеши создаются через тот же session client, принадлежат доменному Engine и могут переживать отдельные операции. Домен управляет их содержимым и инвалидацией. При runtime failure Engine уничтожается; его handles нельзя использовать с другим runtime. Автоматическое восстановление устройства и эпохи кешей не введены.

При отключении сессии сбрасываются scratch handles и OS lease. Глобальный CubeCL client/allocator может удерживать context и память драйвера до выхода процесса. `workspaceBytes` считает только capacities scratch, включая все уже использованные layouts; это не суммарная VRAM, resident память или память allocator.

## Числа и диагностика

`compute::numerics` содержит общую F32 семантику `rp`, `grid_quotient`, `hypot` и execution/arithmetic probes. Placement half ties идут к +Infinity, обычная арифметика использует RTE/FTZ. `grid_quotient` восстанавливает canonical tick/1000 без зависимости от погрешности GPU division; четвертьобороты используют точные перестановки, остальные integer-degree углы — общие F32 bits коэффициентов. Block scoring, MST, collisions, pruning и ranking остаются в домене. Деление и sqrt проверяются с учётом обещанной Vulkan точности; diagnostic score tolerance не служит physical tolerance или pruning margin.

`statistics()` сохраняет `initializations`, накопленный `mutexWaitMs`, `workspaceBytes`, device, capabilities, state и типизированный `unavailableReason`; добавляет precision и Float32 properties/independence. Workspace bytes не являются полной VRAM. Подробные доменные счётчики и стадии остаются в solver. Профилирование не включается автоматически. `PCB_F32_SHADER_AUDIT_DIR` сохраняет фактически исполняемые shader bytes; `PCB_F32_NATIVE_CAPTURE_DIR` сохраняет typed local DTO и frame/original locked metadata перед timed solve, не меняя scores/cache keys.

Сохраняются прежние `PCB_BLOCK_BACKEND`, `PCB_BLOCK_GPU_DISABLED`, `PCB_BLOCK_SOLVER_PROFILE`, validation и fault injection flags, лог-теги и `placement-bench` NAPI probe. Новых aliases переменных среды нет; приоритет прежних flags не изменён. Сохранено поведение: только значение `PCB_BLOCK_GPU_DISABLED=1` запрещает инициализацию, а наличие `PCB_BLOCK_SOLVER_PROFILE` включает соответствующие логи.
