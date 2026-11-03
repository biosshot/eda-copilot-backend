# P6: внутреннее ядро спектров и линейной свёртки

Продолжение пункта P6 «FFT/свёртки, нормировка/окна». Исполнитель p6_medium, прежний worktree. Authored resonant runtime принят; общий sampled-model-to-time этим не разрешается.

## Реализация

Создать небольшой внутренний CPU/F64 модуль для равномерно дискретизированных конечных сигналов: complex FFT/IFFT, spectrum с явной нормировкой, rectangular/Hann window, линейная свёртка. Сначала найти существующие реализации/зависимости; переиспользовать подходящий Rust crate, не писать быстрый FFT с нуля. Если нужна зависимость, обосновать её размер/features, лицензию и отсутствие внешнего runtime. Не перестраивать весь addon только ради этого внутреннего модуля.

Зафиксировать знак экспоненты, масштаб forward/inverse, Hz axis и единицы: DFT coefficient, amplitude spectrum и PSD — разные величины. Нельзя назвать модуль squared amplitude PSD без sample-rate/window-energy normalization. Для one-sided real spectra отдельно DC и Nyquist. Hann coherent gain и energy correction не взаимозаменяемы. Zero-padding не улучшает физическое разрешение и не восстанавливает неизвестную полосу.

Свёртка — linear, с padding >=N+M-1, явным выбором discrete sum либо continuous-time approximation с множителем dt. Не вводить автоматическое продолжение частотной модели, периодизацию transient, resampling неравномерной оси, извлечение причинности из произвольных S-таблиц или новые пользовательские DSL методы.

## Приёмка

- Независимый прямой O(N²) DFT и прямая свёртка для коротких frozen arrays; impulse, constant, bin-centred sinusoid, off-bin tone, complex asymmetric signal, odd/even/prime lengths и random deterministic vectors.
- Round-trip, Parseval, частота/фаза/амплитуда, DC/Nyquist, leakage окон, PSD integral и сохранение единиц. Отрицательные dt/NaN/oversize/unsupported shape дают явный отказ до выделений. Fourier padding и circular aliasing проверять раздельно.
- До расчётов записать error scales/gates и bounds памяти; считать input/output/scratch/plans. CPU F64; cancellation/deadline согласовать с existing analysis job mechanisms, без compute-runtime библиотеки.
- Один modest benchmark с direct reference и длинным сигналом <=512MiB/60s; ничего не утверждать о скорости всего SI. Сохранить numeric spectrum/time PNG и исходные массивы, README с контрактом внутреннего модуля и будущими точками подключения.
- Focused Rust tests, регрессии затронутых модулей. Не менять P7/Thermal/placement или уже принятую резонансную модель.

Работа в отдельном electrical signal module и tests/fixtures; common Cargo изменения согласовать с остальными. Heavy slot/jobs1 по общей очереди. Без коммитов/новых агентов; закончить пакетом для review и остановиться. Это завершает внутренний primitive, не весь P6/P11/EMI.
