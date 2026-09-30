# Первый release CubeCL F64 runtime

2026-09-30. Этап: проверить выбранный runtime/упаковку до переноса массового scorer. Переиспользован runtime и static CRT build способ из `archive/cubecl-placement-partial-20260930`, без старого частичного scorer и board/refinement кода.

- `cargo check --offline --manifest-path native/pcb-board-packer/Cargo.toml --features placement-bench` прошёл.
- Release: `cargo build --release --locked --offline --manifest-path native/pcb-board-packer/Cargo.toml --target x86_64-pc-windows-msvc --features placement-bench` с `CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_RUSTFLAGS=-C target-feature=+crt-static` прошёл.
- DLL скопирована как единственный `debugging/cubecl-block-migration-2026-09-30/runtime/packer.node`. `node scripts/experiment-block-gpu-runtime.mjs` прошёл: четыре F64 батча 4097/1/8193/129 элементов, точный результат, RTX 3060 Laptop GPU, одна инициализация runtime и рост/переиспользование output buffer. Данные различимы в F64 и теряют младшие разряды в F32.
- GPU-disabled import и отказ при явном GPU запросе прошли. Искусственная runtime panic перехвачена внутри mutex, процесс остался жив, последующие GPU запросы корректно отклоняются. Это ещё **не проверка полного CPU перезапуска блока**, который будет подключён вместе со scorer.
- Размер тестового `.node`: **16 076 288 байт**; SHA-256 `fd0a069254357961bc9f12c184c92ed647efc53796cf90eb0af0e85ee999108c`. Финальный размер после scorer потребуется проверить снова.
- Source hashes, команды/логи и per-check результаты: `debugging/cubecl-block-migration-2026-09-30/runtime/`, `runtime-check.log`, `runtime-build.log`, `runtime-smoke.log`. Benchmark API существует только с `placement-bench`; ordinary production CPU scorer пока не изменён.

Это историческая проверка инфраструктуры, **не GPU ускорение block solver**. Последующая production сборка, полные score/pruning/shortlist, singles/pairs, recovery и измерения полного цикла описаны в [RESULTS.md](RESULTS.md). Размер и hashes выше относятся только к раннему runtime smoke.
