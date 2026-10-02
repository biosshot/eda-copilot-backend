# Внутренний GPU API

Актуальный контракт adaptive manager, 2026-10-02. `native/pcb-board-packer/src/compute/`
входит в один существующий `.node`; публичные DTO/NAPI контракты solver не меняются.
GPU-код закрыт feature `gpu`, CPU-only сборка не требует GPU runtime или компилятора.

## Выбор исполнения

`GPU auto` сначала проверяет CUDA, затем Vulkan. Это выбор runtime внутри CubeCL;
`PCB_BLOCK_BACKEND`, `PCB_BOARD_BACKEND`, `PCB_POST_PLACE_BACKEND` по-прежнему выбирают
`cpu` / `auto` / `cubecl` на уровне solver. `PCB_GPU_RUNTIME=auto|cuda|vulkan` позволяет
диагностически выбрать конкретный runtime; значение по умолчанию — `auto`.
`PCB_BLOCK_GPU_DISABLED=1` отключает GPU целиком. `PCB_GPU_CUDA_DISABLED=1` позволяет
проверить автоматический переход к Vulkan при отсутствии CUDA.

CUDA требует доступного NVIDIA driver, NVRTC и заголовков CUDA/CCCL установленного
Toolkit. В Windows учитывается `CUDA_PATH`, включая расположение DLL в `bin/x64`
CUDA 13. Библиотеки Toolkit не копируются в дистрибутив. Vulkan требует установленного
Vulkan driver/loader. Наличие одного из этих GPU runtime достаточно; без обоих
действует исходный CPU solver. Metal/HIP здесь не реализованы; наличие CubeCL само
по себе не добавляет их поддержку. Проверки на одной Windows машине не являются
сертификацией остальных ОС.

`client::Client` владеет одним выбранным ComputeClient. `with_client!` выбирает
конкретный runtime при запуске тех же исходников ядра; алгоритмы scoring не
дублируются. GPU handles никогда не переходят между runtime. Backend выбирается
при инициализации, не меняется посреди поиска. Недоступный/несовместимый CUDA
отклоняется до начала solver GPU-работы, после чего проверяется Vulkan.

## Численный контракт

Оба runtime проходят реальные пробы placement rounding, signed FTZ, отсутствия
неявного FMA, division/sqrt bounds. CPU эталон пробы вычисляется во время исполнения,
с black-box операндами и scoped float environment, а не constant folding компилятора.
Численный контракт завершённой [F32 миграции](F32_MIGRATION_ROADMAP.md) сохраняется.

`PcbRuntime` исправляет исполняемый SPIR-V: RTE, signed FTZ через bit operations,
SignedZeroInfNanPreserve, NoContraction, запрет F64/fast math. CUDA runtime использует
закреплённый CubeCL CUDA patch: `--fmad=false --ftz=true --prec-div=true --prec-sqrt=true`
и отключённый fast math CppCompiler. Политика и build fingerprint входят в kernel ID.
`PCB_F32_SHADER_AUDIT_DIR` сохраняет SPIR-V для Vulkan и CUDA source для CUDA.

## Очередь, CPU и владение

`gpu::with_batch(requirements, class, work, operation)` получает FIFO admission и
собственный scratch до завершения readback. Process-wide depth начинается с 4,
адаптируется в диапазоне 1..8 по throughput/latency и исключает окна холодной
компиляции. Изменение depth не требует полного опустошения очереди. Это число
допущенных операций, не число физических GPU-ядер или аппаратных очередей.

Mutex runtime защищает состояние/инициализацию, не весь upload/dispatch/readback.
Board evaluator извлекается из доменного пула на время запроса, затем возвращается;
его изменяемые caches не разделяются между одновременными jobs. Между процессами
действует scoped OS lease на весь исходный solver call; idle runtime не удерживает
lease. Это сериализация процессов, а не отдельный daemon или глобальный RPC broker.
ОС не предоставляет контракт строгой FIFO-очереди между процессами.

Пакетные block jobs и независимые board beam lanes используют bounded CPU budget.
Ожидающий GPU поток отдаёт CPU permit другому job; резерв ограничен восемью стеками.
Refiner использует producer/consumer pipeline с общим CPU бюджетом, включая один
CPU-поток. Зависимые поисковые шаги остаются последовательными. Если независимой
работы нет, ожидание само по себе не может дать ускорение.

## Память и восстановление

Headroom берётся из Vulkan memory budget или CUDA mem_get_info, обновляется с кэшем
250 ms и динамическим запасом 5%. Неизвестный бюджет остаётся неизвестным.
Размеры батчей учитывают этот сигнал и переиспользуемый scratch; при давлении
освобождаются idle buffers и запрашивается best-effort cleanup allocator.
Оценки не являются глобальной гарантией VRAM или резервированием чужих процессов.
Сохраняется минимальный батч в один кандидат, чтобы собственный reusable allocator
не приводил к вечному ожиданию. Реальный отказ allocation — ошибка исполнения.

В `auto` занятое устройство/очередь может привести к выбору CPU до начала GPU call.
Явный `cubecl` ожидает admission; из-за нагрузки он не меняет backend посреди поиска.
При настоящем GPU сбое, включая OOM, все частичные результаты вызова отбрасываются,
выполняется полный CPU F32 повтор с исходного входа; refiner получает полный timeout.
Повреждённый runtime отключается до конца процесса. Уже запущенные операции
завершают ожидание, sibling-результаты после отключения не публикуются. Автоматических
GPU повторов после частичного исполнения нет. `PCB_GPU_FAIL_ALLOCATION=1` — только
диагностическая инъекция этой ветви, без искусственного заполнения всей VRAM.

## Диагностика

`statistics()` содержит backend, capabilities, state, unavailableReason, queue,
processLease, memory, memoryTrims, workspaceBytes и startup/cache metrics.
`cudaCompilation.nativeCompilationMs` учитывает NVRTC; `kernelPreparation.firstLaunchHostMs`
включает подготовку первого запуска и пересекается с временем компиляции — эти
времена нельзя складывать. Vulkan disk cache использует build/device/options keys;
CUDA kernel IDs также включают fingerprint для безопасности upstream PTX cache.

`[block-cpu-scheduler]` и `[board-cpu-scheduler]` показывают active CPU limit/peak и
suspended worker time. Readback и suspended time включают ожидание, а не только
копирование байтов. Итоговую скорость оценивают по wall time полного исходного
вызова, с указанием состояния shader cache и качества итоговой геометрии.

CUDA uses at most eight backend allocator streams, matching the admission ceiling;
it does not retain the upstream default of 128 pools for short-lived beam threads.
At board phase barriers, obsolete frame/resident handles and idle scratch are
released before local/repair work. CUDA cleanup visits all allocator pools.
A failed allocation is latched inside the backend before an unbound handle can
reach launch/write; readback delivers the error to full CPU recovery.
