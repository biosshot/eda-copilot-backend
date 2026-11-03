# P6b frequency networks — verification, 2026-10-23

Реализация по обновлённому `next-task-frequency-networks.md`, включая универсальный
сетевой `a.model()` и Attenuator Pad. Передача на ревью; коммит не создан, P7/PI/SI
не начаты. База HEAD `34e61a1e840e5bb61a3bafc4c61513d0a277bf0a`, ветка `feat/analyze`.

## Что реализовано

Существующий production DSL/process/shared addon → Rust CPU/F64: нормализованный
S/Z/Y Resource, ограниченный Touchstone, преобразования/перенормировка и попарная
композиция multiport. Фабрика/прямой Resource/каталог используют общую структурную
валидацию и ранние ограничения. LocalModel и его отрицательные проверки сохранены.
Независимые экземпляры, nested mapping, явные нагрузки, overlap/foreign handles,
пассивность, полоса, сингулярности, immutable/reusable results и их бюджет проверены.
Один связный common-return граф; одинаковая нормировка не соединяет отдельные возвраты.

Подробные [формулы, контракт, поддержанный поднабор и caps](../reference/frequency-network-semantics.md).
Перенормировка не заменяет reference metadata. Идеальный thru не обязан проходить
через сингулярную Z. Нет gmin, экстраполяции, matched-заглушек или проекции пассивности.

Нормированный остаток <=1e-8, condition infinity-norm <=1e12. Пассивность:
max singular value <=1+1e-9 через положительность Hermitian power form. Тестовые
complex допуски: 1e-8 relative + 1e-10 absolute для S; абсолютные floors 1e-10 Ohm
и 1e-10 Siemens для Z/Y. Независимые аналитические оракулы проверяют комплексные
коэффициенты, а не один модуль и не только round-trip.

## Фактически выполненные проверки

Windows x64; Node v26.5.0, rustc/cargo 1.98.1. Все перечисленные команды: **exit 0**.

```powershell
$env:PCB_BLOCK_BACKEND='cpu'
$env:PCB_BOARD_BACKEND='cpu'
$env:PCB_POST_PLACE_BACKEND='cpu'
cargo test --release --locked --manifest-path native/analyze/Cargo.toml --test frequency --test electrical --test dc --test conduction
npm run native:build
npm run typecheck
npm run build
node --import tsx --test --test-concurrency=1 tests/analysis-dsl.test.ts tests/pcb-analysis-dc.test.ts tests/pcb-analysis-dc-ports.test.ts tests/pcb-analysis-dc-audit.test.ts tests/pcb-analysis-electrical.test.ts tests/pcb-analysis-rlc-audit.test.ts tests/pcb-analysis-frequency.test.ts tests/pcb-analysis-frequency-audit.test.ts tests/pcb-analysis-conduction.test.ts tests/pcb-analysis-geometry.test.ts tests/pcb-analysis-runtime.test.ts tests/pcb-native-addon.test.ts tests/public-api.test.ts
node --test scripts/test-pcb-analyze.test.mjs
npm run test:package
npm run test:pcb-analyze -- BandAmpV5 STM32G0_CAN_RS485 --name network
git diff --check
```

- **64 Rust**: 15 frequency + 17 electrical/E1/E2 + 12 DC + 20 conduction/M1.
- **166 Node**: новый production путь, публичные типы, legacy LocalModel/DC/RLC,
  E1–E3, D1–D3, геометрия/сетка, runtime lifecycle, addon/API/placement smoke.
- **4 launcher** теста. Native build, typecheck/build прошли.
- Package: исходный установленный API/упаковка/восемь импортов, новый частотный
  сценарий и типизированная фабрика проверены в normal, GPU-disabled и unavailable
  drivers режимах. Установленный binary совпадает с текущим по SHA256.
- Аудит негативных данных: 15 подмен проверяют S12/S21, source/output reference,
  порядок портов, коэффициенты/хеши источников, частоты/экстраполяцию, nature,
  interpolation/conditional, удалённые plots, массивы plots и fail→continue.
- Ресурсные отказы до Array.from/токенизации: bytes, headers, F×N², output/work,
  native memory/time. Singular/passivity/band ошибки не дают нулевую successful матрицу.
- Timeout/cancel/report budget завершаются контролируемо, следующий запуск работает.
  Расширенная проверка result.network не копирует таблицы всех предков в provenance.
- CR/LF/CRLF диагностика, signed-zero DC, разные return groups и чрезмерный gain
  проверены отдельно. Отдельные возвраты явно unsupported, а не соединены через R0.

Логи: `debugging/analysis-dsl/frequency-{rust-final,native-final,typecheck,build,node-final,launcher,package,fixtures}.log`.

## Исполненные сценарии и визуальная проверка

Оба новых named fixture прошли текущую production-сборку: **12 cases, 168 plot
arrays**, 10 намеренных fail и 12 последующих pass; required unknown отсутствуют.
По пять самостоятельных RF-эталонов и одной отдельной модели участка на страницу.

| Fixture | Cases / plot arrays | Wall | Peak worker RSS | Max S complex abs error | Max normalized residual |
| --- | --- | --- | --- | --- | --- |
| BandAmpV5 | 6 / 84 | 0.794 s | 93.60 MiB | 1.110e-16 | 5.176e-17 |
| STM32G0_CAN_RS485 | 6 / 84 | 0.811 s | 102.48 MiB | 1.110e-16 | 3.925e-17 |

Сохранены точные `.js`, исходный Resource JSON, реальные байты `.s2p`, resolved
каталог, локальная таблица, hashes, implementation snapshot, JSON/audit и HTML.
Проверка совпадения текстового `.s2p` с JSON учитывает только LF/CRLF; в solver
передаются именно реальные bytes файла, они сохраняются отдельно без преобразований.
RSS — OS process high-water через native call, с Node/предыдущими вызовами; не
строгий лимит OS памяти и не только память текущего solve.

- [BandAmpV5 HTML](../../../debugging/pcb-analyze/BandAmpV5/runs/2026-10-23T08-34-26-611Z-network-DCElkL/index.html), [JSON](../../../debugging/pcb-analyze/BandAmpV5/runs/2026-10-23T08-34-26-611Z-network-DCElkL/network.json), [audit](../../../debugging/pcb-analyze/BandAmpV5/runs/2026-10-23T08-34-26-611Z-network-DCElkL/audit.json).
- [STM32G0_CAN_RS485 HTML](../../../debugging/pcb-analyze/STM32G0_CAN_RS485/runs/2026-10-23T08-34-27-953Z-network-04Tda5/index.html), [JSON](../../../debugging/pcb-analyze/STM32G0_CAN_RS485/runs/2026-10-23T08-34-27-953Z-network-04Tda5/network.json), [audit](../../../debugging/pcb-analyze/STM32G0_CAN_RS485/runs/2026-10-23T08-34-27-953Z-network-04Tda5/audit.json).

[Обзор](../../../debugging/pcb-analyze/network/index.html).
[Сводные результаты и hashes](../../../debugging/analysis-dsl/frequency-results.json).
[Восемь скриншотов](../../../debugging/pcb-analyze/network/screenshots/).
В браузере открыты обе финальные страницы; проверены passive/active magnitude,
phase, real/imaginary, S11/S12, source mapping, 50→75 Ohm output reference,
читаемый expected fail→pass и отдельные низкочастотные Z-гипотезы.

Контакты проверены по V2: BandAmpV5 U1.6/U1.2 и U1.3/U1.4; STM32 U2.1/U2.2
и U2.5/U2.2. Выбранные сетевые таблицы — явно синтетические гипотезы. Контроллер
не объявлен GHz-усилителем; RF-эталоны показаны отдельно от его участка 1–100 kHz.
Медная сетка и автоматическая физическая привязка здесь не используются.

Прямой [пример из раздела 7](../../../tests/fixtures/analysis-frequency/attenuator-example.js)
выполнен без переписывания DSL: **S21=0.25** на 1 GHz, check pass; два экземпляра
одного factory model. [Exact code/result/hashes](../../../debugging/analysis-dsl/frequency-example.json).

## Версии и границы регрессий

Native SHA256: `58484c2031ea0a047232c3dde1ca3e1413e6ff9329c2535d2643271f2aea694f`.
Archive SHA256: `34a51d7c6240874ecfafdafd45ca6778ed64fa1337fcd1a0bb2280a4015e121e`.
Установленный consumer: `C:\Users\kiril\AppData\Local\Temp\eda-backend-package-xZb2pb`.

[Source parity](../../../debugging/analysis-dsl/frequency-source-parity.json):
**80 core files** FEM/DC/geometry/import, MNA network/solve/waveform, placement
и независимых адаптеров идентичны HEAD после нормализации EOL. electrical::Session
идентичен после удаления одной декларации нового модуля. Общие context/executor/
transport изменены добавочными ветвями и проверены focused suite; их полную
побайтовую идентичность не заявляем. JSON плат, их существующие DC/RLC `.js` и
физические inputs не менялись. Старые восемь больших DC и три RLC полного корпуса
не повторялись; их исторические binary hashes не заменены текущим.

Не выполнялись Linux/macOS, CI и весь `npm test`; локальные результаты не
объявляются такими проверками. Это ограниченный Touchstone/S/Z/Y блок, не весь
стандарт и не завершение P6. Нельзя импортировать временной отклик, выполнять
fitting/FFT/TDR/PCB extraction, доказать причинность/устойчивость во всей полосе
или получить физическую AC-карту платы. Пассивность проверена только в вычисленных
точках; интерполяция остаётся явной conditional с ресурсами и интервалами.

[Diff для ревью](../../../debugging/analysis-dsl/frequency-review/changes.patch) и
[manifest](../../../debugging/analysis-dsl/frequency-review/manifest.json).
Пользовательские изменения задания/плана и `native/analyze/debugging/` сохранены;
в implementation patch они не включены. Коммит/стейджинг не выполнялись.
