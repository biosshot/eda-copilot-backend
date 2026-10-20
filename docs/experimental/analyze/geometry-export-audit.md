# Геометрия EasyEDA → RoutingBoard: проверка покрытия

Дата: **2026-10-19**. Проверены локальный код, локальная копия SDK-документации и три сохранённых autoroute JSON. Новый экспорт из запущенного CAD не выполнялся. Это аудит текущих путей, а не гарантия поведения всех версий EasyEDA.

HEAD при проверке: EasyEDA Copilot `12b265a051c6977d5b308329a105aa2c4876b6e2`, router `eacb66ea46723e9b779de9ba0ee72fd21c086971`. Описывается прочитанное состояние рабочих файлов. [Черновик V2](routing-board-v2-draft.md), [предыдущий аудит](routing-board-audit.md).

## Источники

- [Контракты router](../../../../copilot-router/src/core/contracts.ts).
- [Autoroute adapter](../../../../easyeda-copilot/mcp/src/routing/easyeda-autoroute-adapter.ts): `decodePath`, `decodePolygon`, `importedPadShape`, `importedZones`, `importEasyEdaAutorouteJson`.
- [Экспорт из расширения](../../../../easyeda-copilot/extension/src/mcp-client.ts): `exportRoutingInput`; [вызов адаптера](../../../../easyeda-copilot/mcp/src/routing/routing-operation.ts).
- [Прямое чтение PCB](../../../../easyeda-copilot/extension/src/eda/pcb.ts): `getPcbRaw`, `readBoardPolygon`, `polygonSourceToPoints`, `convertSourceArray`; [RawPcb](../../../../easyeda-copilot/shared/types/pcb/raw.ts).
- [Native DRC](../../../../easyeda-copilot/shared/types/pcb/drc.ts), [DRC adapter](../../../../easyeda-copilot/mcp/src/routing/easyeda-drc-adapter.ts).

## Что действительно есть в сохранённых JSON

| Файл | Наблюдение |
| --- | --- |
| [ESC G431](../../../../copilot-router/tests/pcb-routing/esc_g431_easyeda/easyeda-routing-input.json) | 6 tracks, 3 vias, 0 fillRegions. У круглых площадок есть `diameter` и путь из двух полуокружностей: `[x,y,-180]`. |
| [STM32G0 CAN/RS485](../../../../copilot-router/tests/pcb-routing/stm32g0_can_rs485_easyeda/easyeda-routing-input.json) | 3 fillRegions с `path`, `layer`, `net`; нет явного признака Pour/Fill в рассмотренных записях. Круглые площадки также представлены дугами. |
| [Maverick H743](../../../../copilot-router/tests/pcb-routing/maverick_h743_easyeda/autoroute.json) | Есть дуги около −90° в `boardOutline.path`. Две prohibitedRegions; рассмотренная круглая область задана двумя дугами +180°. |

Во всех трёх файлах есть `constraintRegions`; отдельных корневых `circles`, `arcs`, `holes` нет. Это **не означает отсутствия окружностей или отверстий на платах**: окружности уже видны внутри путей; смысл каждой prohibitedRegion и соответствие исходным отверстиям без CAD-сопоставления не доказаны. Дуги в дорожках этих примеров не подтверждены: поддержка таких путей видна по коду адаптера, но отдельный реальный пример ещё нужен.

## Как декодируется геометрия

Autoroute adapter читает `[x,y]` как точку, а третий элемент очередной точки — как угол дуги от предыдущей точки. При отражении Y меняется знак угла. Дуги превращаются в ломаные с целевой геометрической погрешностью `ARC_TOLERANCE_MM = 0.001`; пределы — 1024 сегмента на дугу и 16384 точки пути. Превышение лимитов даёт ошибку декодирования; импорт некоторых объектов после этого пропускается с диагностикой.

Одна дуга ±360° либо дуга с совпадающими концами не поддерживается этим декодером. Круг из двух полуокружностей поддерживается. Круглая площадка с положительным `diameter` сохраняется как `RoutingPadShape.kind = 'circle'`, без преобразования её пути в polygon.

Для прочих площадок с дугами применяется специальная обработка: выпуклые дуги огибаются касательными, вогнутые — хордами, чтобы **не уменьшать препятствие для роутера**. Полученный контур не следует выдавать за точную физическую медь. Точность, исходные дуги и характер приближения в результирующем RoutingBoard отдельно не записываются.

`decodePolygon` понимает один путь либо список «внешний контур, затем отверстия». Это не универсальное представление произвольного набора островков с вложенностью. Результат реальной заливки нужно нормализовать в несколько областей с явно назначенными отверстиями.

Прямой путь RawPcb устроен иначе. `convertSourceArray` сохраняет команды `CIRCLE`, `ARC`, `CARC`, `R`, `C`, `Q` и переводит координаты в mm. Наличие ветки конвертера ещё не доказывает корректность всех команд на реальном экспорте. В частности, `readBoardPolygon` использует другой, упрощённый `polygonSourceToPoints`: окружность там превращается в 24 точки, а для `ARC`/`CARC` код собирает пары чисел без полноценного вычисления дуги. Затем выбирается только наибольшая polyline границы. Поэтому готовый `RawPcb.board` тоже нельзя считать проверенным полным контуром платы с вырезами.

## Матрица потерь и источников дополнения

| Данные | RoutingBoard / текущий autoroute adapter | Прямое чтение и необходимое действие для V2 |
| --- | --- | --- |
| Внешний контур, дуги | Контур превращается в points | По уточнению пользователя использовать контролируемую аппроксимацию в существующих точках; не брать упрощённый RawPcb outline без доработки |
| Вырезы, отдельные тела платы | `cutouts: []`; единственный внешний контур | Отдельно проверить native границы, вложенность и физические вырезы; keepout не заменяет отверстие |
| Дорожки | Один `widthMm` на объект; разные объекты одной сети могут иметь разные ширины; дуги аппроксимируются | Сохранить геометрию через контролируемую аппроксимацию, учитывать cap/join либо нормализованный медный контур, tapered/custom copper |
| Площадки | Круг/rect/polygon; один shape на все layers; импорт `hole` не заполняет | RawPcb читает hole/offset/rotation и native ID; нужны plating, корректные единицы, standalone pads, при необходимости послойные формы |
| Переходы | Размеры читаются; adapter ставит top→bottom и `through` | SDK имеет via type и имя blind-via rule, но этого без разрешения правила недостаточно для span; не угадывать blind/buried/micro |
| Pour/Fill | `fillRegions` → RoutedZone с `connection: solid`; вид источника не сохранён | Чтение Pour, Poured и Fill отдельно; сохранять связи и происхождение |
| Реальная медь Pour | Не читается как отдельный CAD-результат | `getState_PourFills()` уже используется RawPcb; сохранить parent ID, fragment ID, слой, геометрию, статус чтения |
| Постоянный Fill | Семантика не различается | RawPcb принудительно пишет `fill: true`, `lineWidth: 0`; SDK имеет `getState_FillMode` и `getState_LineWidth`, их нужно исследовать и переносить |
| Штрихи/контуры заливки | Нет отдельного носителя исходной семантики | Poured fragment содержит `fill`, `lineWidth`, `path`; stroke нельзя читать как заполненную площадь или отверстие только по boolean |
| Правила Pour | Тип имеет часть полей, adapter их не заполняет | Native Pour priority/fill method/preserve silos + DRC Copper/Plane presets + pad overrides; проверить фактическое разрешение наследования |
| Keepouts/constraint regions | prohibitedRegions → запрет tracks/vias/zones одновременно; constraintRegions этим adapter не переносится | Проверить исходную семантику; хранить отдельные ограничения, а не приписывать всем областям один запрет |
| Copper graphics/text | Нет отдельного полного пути импорта | Проверить линии, дуги, окружности, polyline, текст на copper layers, teardrops и custom copper; нормализовать их физическую площадь либо сообщать о неполноте |
| Netless copper | Track/via/импортируемая zone требуют net; могут быть пропущены | Не исключать медь из физической модели из-за неизвестной сети; различать no-net и unknown |
| Компоненты | Положение/сторона есть, обозначение корпуса в RoutingComponent отсутствует | Autoroute components содержит `footprint`; сохранить как CAD-обозначение, не считать автоматически стандартом корпуса |
| Stackup/materials | Тип есть, текущий adapter не заполняет | DSL/профиль производителя с указанием происхождения; сведения о порядке слоёв ещё не являются физическим stackup |
| Идентификация/полнота | Некоторые IDs отсутствуют; warnings не дают полного покрытия | Native ID + document/snapshot; отчёт чтения по семействам объектов и список нераспознанного |

## Что SDK подтверждает, а что ещё нужно проверить

В локальных справочниках есть:

- [Pour](../../../../easyeda-copilot/mcp/docs/execution/easyeda-api/references/classes/IPCB_PrimitivePour.md): complex polygon, layer, net, line width, fill method, priority, preserve silos.
- [Poured](../../../../easyeda-copilot/mcp/docs/execution/easyeda-api/references/classes/IPCB_PrimitivePoured.md) и [его фрагмент](../../../../easyeda-copilot/mcp/docs/execution/easyeda-api/references/interfaces/IPCB_PrimitivePouredPourFill.md): ссылка на Pour, ID фрагмента, path/fill/lineWidth.
- [Fill](../../../../easyeda-copilot/mcp/docs/execution/easyeda-api/references/classes/IPCB_PrimitiveFill.md): complex polygon, fill mode, line width, layer/net/ID.
- [Pad thermal overrides](../../../../easyeda-copilot/mcp/docs/execution/easyeda-api/references/interfaces/IPCB_PrimitivePadHeatWelding.md): connection method, divergence spacing/width/angle.
- [Via](../../../../easyeda-copilot/mcp/docs/execution/easyeda-api/references/classes/IPCB_PrimitiveVia.md): via type, blind-via rule name, диаметры и ID.

`PcbDrcBundle` сохраняет сырые presets и назначения, включая Copper Safe Spacing, Plane Safe Spacing, Copper Zone, Plane Zone, Blind/Buried Via. Нынешний `mergeEasyEdaDrcIntoRoutingRules` не превращает это в полные effective refill rules. Обратный код `assignCopperZonePreset` знает часть полей тепловых подключений, но он не является полноценным импортёром правил и не доказывает все значения и приоритеты EasyEDA.

## Проверки перед утверждением схемы и реализацией

Это будущий план проверки, **не выполненный тестовый набор**. Для каждой строки нужны исходная CAD-плата, autoroute JSON, native snapshot, ожидаемая геометрия и результат будущего V2-импорта.

| Группа | Контрольные случаи |
| --- | --- |
| Кривые и преобразования | ±90/180°, большая дуга, полный круг, bottom mirror + rotation, большие координаты, пределы точности |
| Медные пути | Прямой/дуговой track, соединение разных ширин, замкнутый stroke, caps/joins, copper polyline/text |
| Граница | Скругления, круглая плата, несколько вырезов, паз, NPTH, несколько независимых тел; запретная область без физического отверстия |
| Площадки/vias | SMD/TH/standalone, round/slot, смещение сверления, custom pad, unknown net, послойные различия, blind/buried/micro |
| Зоны | Pour и Fill с одинаковым контуром; solid/outline/hatch по доступным режимам; островки и отверстия; пустой результат; отсутствующий Poured; orphan Poured |
| Правила | Наследование global→class→net и overrides, SMD/TH/via thermals, same-net overlaps, разный/одинаковый priority, island removal, edge/keepout |
| Согласованность | Изменение платы между чтениями; устаревший Poured; ошибка API; unsupported command; физически ошибочная, но корректно прочитанная медь |

Проверять нужно не только количество объектов: площадь и границы меди, отверстия, контакты, сети, порядок слоёв и отсутствие двойного учёта. Сравнение с картинкой preview полезно, но не доказывает сохранение электрической связности.
