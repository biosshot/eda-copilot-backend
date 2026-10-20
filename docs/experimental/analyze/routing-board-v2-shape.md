# RoutingBoardV2: читаемая структура TypeScript

Обновлено **2026-10-19**. **Текущая структура предварительно зафиксирована пользователем как рабочий контракт; требует перепроверки.** Это документация будущего API. [Полный JSON-пример](routing-board-v2-example.json) — искусственные данные, не экспорт реальной платы. [Подробности и ограничения](routing-board-v2-draft.md).

При итоговом пересмотре всего плана обязательно проверить преобразование native PourFills в `PolygonMm[]`: один элемент описывает заполненную область, `outer` — её внешнюю границу, `holes` — вырезы в меди этой области, которые не обязательно являются сверлениями платы. Это нормализованное представление, а не готовые поля ответа EasyEDA. Проверить островки, вложенные контуры, strokes, thermals, точность аппроксимации и доступность остальных полей на реальном экспорте. Предварительная фиксация не означает, что эти проверки уже пройдены.

По последнему уточнению **`schemaVersion` и `importInfo` убраны**. V2 остаётся рабочим именем типа на время обсуждения. Ошибки чтения можно возвращать обычными diagnostics операции; для этого не нужен новый раздел внутри платы.

## Что сохраняем и что добавляем

Сохраняем существующие `PointMm`, `PolygonMm`, формы площадок, `outline`, `cutouts`, `points`, слои, stackup и routing rules. **Дуги аппроксимирует CAD-адаптер; отдельного типа кривых не будет.** Простые circle/oval/round-rect площадки остаются существующими примитивами; сложная геометрия выражается точками.

Главное расширение зоны — `kind` и `filledPolygons`. **Да, для Pour берём настоящую native-заливку EasyEDA**: `PrimitivePour` даёт контур и свойства, `PrimitivePoured.getState_PourFills()` — результат заполнения. `PrimitiveFill` читается отдельно. Полигоны нормализуются с учётом native fill mode, line width, отверстий и аппроксимации дуг. Источником служит CAD, не refill нашего router.

Ниже уже существующие типы импортируются, а не объявляются заново. Относительный путь обозначает проверенную текущую декларацию; это не предложение runtime-зависимости backend от пакета router.

```ts
import type {
  PointMm, PolygonMm, RoutingBoard, RoutingComponent, RoutingPad,
  RoutingPadShape, RoutingRules, RoutedTrack, RoutedVia,
  RoutedZone, RoutingCopper,
} from '../../../../copilot-router/src/core/contracts';

// НОВОЕ: фактическая медь CAD на одном слое.
type FilledPolygons = {
  layer: string;
  source: 'cad';
  nativePrimitiveIds?: readonly string[];
} & (
  | {
      status: 'available';
      polygons: readonly PolygonMm[];
      freshness: 'current' | 'stale' | 'unknown';
    }
  | {
      status: 'missing' | 'failed' | 'unsupported';
      reason: string;
    }
);

// НОВОЕ: Pour сохраняет ВСЕ существующие поля RoutedZone:
// outline, layers, net, priority, clearanceMm, minThicknessMm,
// connection, fill (solid/hatch), padConnection, removeIslandsBelowMm2.
type PourZoneV2 = Omit<RoutedZone, 'id'> & {
  id: string;
  kind: 'pour';
  filledPolygons: readonly FilledPolygons[];
  refillRules: {
    status: 'complete' | 'partial' | 'unknown';
    ruleSetId?: string;              // ссылка на rules.refill
    unresolved: readonly string[];  // чего не смогли узнать из CAD
  };
};

// НОВОЕ: постоянная медь использует ту же базовую геометрию.
// У неё нет правил перестроения Pour.
type FillZoneV2 = Pick<RoutedZone, 'net' | 'layers' | 'outline'> & {
  id: string;
  kind: 'fill';
  filledPolygons: readonly FilledPolygons[];
};

type RoutedZoneV2 = PourZoneV2 | FillZoneV2;

// НОВОЕ: различия CAD rules, которые не выражаются одним clearanceMm.
// Это дополнение к прежним полям zone, не их вторая копия.
type RoutingRulesV2 = RoutingRules & {
  refill?: readonly {
    id: string;
    clearances?: readonly {
      obstacle: 'track' | 'smd-pad' | 'th-pad' | 'via'
        | 'zone' | 'edge' | 'hole' | 'keepout';
      netRelation: 'same-net' | 'different-net' | 'any';
      mm: number;
      nets?: readonly string[];
      layers?: readonly string[];
      region?: PolygonMm;
    }[];
    viaConnection?: RoutedZone['padConnection'];
    thPadConnection?: RoutedZone['padConnection'];
    preserveIslands?: boolean;
    hatchOrigin?: PointMm;
  }[];
};

type RoutingComponentV2 = RoutingComponent & {
  id?: string;
  footprintName?: string;  // напрямую из getState_Footprint()?.name
  packageName?: string;    // распознанный конкретный вариант корпуса
  bodySizeMm?: { x: number; y: number; z: number }; // из каталога/DSL
  // УЖЕ есть: designator, at, rotationDeg, side, bodyOutline, bounds.
  // x/y — размеры тела в локальных осях, z — высота над поверхностью платы.
};

type RoutingPadV2 = Omit<RoutingPad, 'id' | 'component'> & {
  id: string;
  component?: string;     // отсутствует у standalone pad
  shapesByLayer?: Readonly<Record<string, RoutingPadShape>>;
  zoneConnection?: RoutedZone['padConnection'];
  // hole, offsets, rotation и plated УЖЕ есть в RoutingPad.
};

// net можно не знать, но физическая медь всё равно остаётся на плате.
// Это единственное изменение формы track/via в данном примере.
type RoutedTrackV2 = Omit<RoutedTrack, 'net'> & { net?: string };
type RoutedViaV2 = Omit<RoutedVia, 'net'> & { net?: string };

type RoutingCopperV2 = Omit<RoutingCopper, 'tracks' | 'vias' | 'zones'> & {
  tracks: readonly RoutedTrackV2[];  // прежние points + widthMm
  vias: readonly RoutedViaV2[];     // прежние from/to, diameters, type
  zones: readonly RoutedZoneV2[];
};

type RoutingBoardV2 = Omit<RoutingBoard,
  'components' | 'pads' | 'rules' | 'copper'
> & {
  components: readonly RoutingComponentV2[];
  pads: readonly RoutingPadV2[];
  rules: RoutingRulesV2;
  copper: {
    fixed: RoutingCopperV2;
    editable: RoutingCopperV2;
  };

  // Только независимые отверстия. Pad/via hole повторно сюда не копируем.
  holes?: readonly {
    id: string;
    at: PointMm;
    hole: NonNullable<RoutingPad['hole']>;
  }[];

};
```

Из исходного `RoutingBoard` через `Omit` автоматически остаются:

```ts
// Показаны для чтения. Это прежние поля, не новые определения.
// outline: PointMm[];
// cutouts: PointMm[][];
// layers: RoutingLayer[];
// nets: RoutingNet[];
// keepouts: RoutingKeepout[];
// stackup?: RoutingStackup;
```

`stackup` может прийти из DSL/профиля производителя, а в будущем — из CAD API. Меняется источник заполнения существующего поля. `keepouts` сохраняет прежние polygon/layers/forbid. Дополнительные неизвестные native правила отмечаются в diagnostics операции и `refillRules.unresolved` до уточнения их модели.

## Компоненты и приближённые корпуса

Компоненты находятся в **`board.components`**; этот массив уже существует в RoutingBoard. Теперь JSON-пример содержит U1 с названием footprint, распознанным корпусом, размером тела и `bodyOutline`. Размеры заданы исключительно для демонстрации, pads и электрическая модель U1 в этом примере не описаны.

Поток данных: native footprint name → распознавание конкретного корпуса → получение размеров из каталога либо DSL → построение прямоугольного тела с учётом at/rotationDeg/side. Для теплообмена это приближённый параллелепипед с высотой. Если название неоднозначно, `packageName`/`bodySizeMm` остаются незаполненными до уточнения. `bodySizeMm` — optional уточнение: можно передать только имя и разрешить геометрию на стороне анализа.

Native bbox компонента может включать площадки/графику и не даёт автоматически размер пластикового тела; высоты у 2D bbox нет. Начало footprint тоже не обязательно совпадает с центром корпуса. При известном положении тела используем существующий `bodyOutline`; при построении из каталога требуется корректное смещение тела относительно anchor. Неизвестное смещение — явное допущение расчёта.

Для thermal нужны также тепловыделение, теплопроводность материалов и тепловые связи компонента с платой/окружением. Размеры внешнего тела позволяют разместить геометрию, но не определяют эти свойства. Мощности, охлаждение и выбранные модели задаются сценарием/библиотекой; добавлять их обязательными полями RoutingBoard сейчас не предлагается.

## Что переиспользуем из RawPcb

Сейчас [RawPcbPolygon](../../../../easyeda-copilot/shared/types/pcb/raw.ts) устроен так:

```ts
type RawPcbPolygon = {
  net: string;
  layer: string;
  fill: boolean;
  lineWidth: number;
  sources: (number | string)[][]; // native-команды, координаты уже в mm
};
```

Путь чтения: [getPcbRaw](../../../../easyeda-copilot/extension/src/eda/pcb.ts) → `sources` → [polygonRings / parsePolygonSource](../../../../easyeda-copilot/mcp/src/pcb-preview/parser.ts) → [renderPolygon](../../../../easyeda-copilot/mcp/src/pcb-preview/renderer.ts). Следовательно, пользователь прав: RawPcb сохраняет команды EasyEDA, а разбор выполняется на стороне Preview.

Предлагается переиспользовать native-чтение, перевод единиц и проверенные части декодера; перенести их в адаптер общей платы. После декодирования получаем `filledPolygons: PolygonMm[]` по слоям. Preview и анализ используют уже готовую геометрию. Сохранять native `sources` вторым обязательным форматом в RoutingBoard не требуется.

**Декодер Preview пока нельзя целиком считать готовым нормализатором физической меди.** При чтении кода обнаружено:

- `CIRCLE` всегда даёт 24 точки; дуги — от 4 до 64 сегментов по углу, без задания максимальной ошибки в mm.
- `roundedRectPoints` возвращает углы прямоугольника даже при положительном радиусе скругления.
- В `flattenQuadraticBezier` выражение координаты x использует `p2.y` вместо `p2.x`.
- [buildIslands](../../../../easyeda-copilot/mcp/src/pcb-preview/geometry.ts) отбрасывает cutouts площадью меньше 0.2 mm² либо с малым отношением площади к bbox. Для расчёта такие отверстия могут быть существенны.
- Renderer передаёт `lineWidth` SVG stroke; в массиве колец нет готовой площади этого stroke. Для анализа её нужно построить и объединить с заполнением с правильной семантикой caps/joins.

Эти находки записаны для будущей доработки; код Preview сейчас не исправляется. Проверенное отображение обычной платы подтверждает полезность существующего пути, но не полноту геометрии всех поддерживаемых команд.

Дополнительно выполнены три проверки существующих функций в памяти, без изменения исходников: CIRCLE дал 24 точки; Q с ожидаемым концом `(3,4)` вернул `(4,4)`; квадратное отверстие площадью 0.04 mm² внутри внешнего контура было удалено `cleanPolygonRings`. Это подтверждает конкретные ограничения, а не заменяет проверку native-экспорта на контрольных платах.

## Можно ли заполнить все поля

Проверка ниже основана на исходниках и локальном SDK; новый полный экспорт из живого CAD не выполнялся.

| Поля | Доступный источник | Что пока ограничено |
| --- | --- | --- |
| `kind`, `id`, `net`, `layers`, `outline` зоны | Тип native primitive, ID/net/layer/ComplexPolygon у Pour/Fill | Нужна правильная нормализация; orphan Poured требует диагностики |
| `filledPolygons.polygons` | Poured `getState_PourFills()`; Fill `getState_ComplexPolygon()` | Чтение уже есть, конвертер физической меди ещё требуется; RawPcb сам не хранит PolygonMm |
| `nativePrimitiveIds` | ID Pour/Poured/Fill и fragment.id | Текущий RawPcb часть связей теряет; их можно сохранить на этапе чтения |
| `status`, `source`, `refillRules.status/unresolved` | Назначает экспортёр по реально прочитанным данным и ошибкам | Это результаты чтения, не значения CAD API |
| `freshness` | При доказательстве актуальности — current/stale, иначе unknown | Проверенного универсального сигнала актуальности Poured пока нет |
| `priority`, `fill`, `preserveIslands` | Pour priority, fill method, preserve silos | Priority/preserve доступны; fill method в SDK имеет тип any, преобразование его вариантов ещё проверить |
| `clearanceMm`, `minThicknessMm`, `padConnection`, `rules.refill` | Native DRC presets/assignments; pad `getState_HeatWelding()` | Полный импорт наследования, spokeCount, hatchOrigin и всех режимов не подтверждён; неизвестное не заполнять догадкой |
| `refillRules.ruleSetId` | Назначает адаптер для нормализованного набора правил | Отсутствует, если набор не удалось восстановить |
| component `id`, `designator`, `at`, `rotationDeg`, `side` | Native component API; часть уже переносится autoroute adapter | Сверить преобразования bottom/rotation; исходный компонент уже есть в модели |
| `footprintName` | `getState_Footprint()?.name`; autoroute component.footprint | Уже читается в `readComponents`, но не переносится текущим RoutingComponent importer |
| `packageName`, `bodySizeMm`, точный `bodyOutline` тела | Распознавание footprint + каталог размеров, явные данные DSL | Прямое надёжное чтение всех этих полей из текущего пути не подтверждено; optional |
| `shapesByLayer` | Только при наличии соответствующей информации native padstack | Полное чтение не подтверждено; optional, общий shape уже есть |
| pad `hole`, via span/type, отдельные holes | Pad API/правила переходов/источник сверлений | Нынешний autoroute импорт неполон; NPTH и span нельзя выводить из keepout/наличия размеров |
| `stackup` | Пока DSL/профиль; будущее CAD API | Физические толщины/материалы текущим импортёром не читаются |
| `keepouts`, локальные ограничения | Autoroute prohibitedRegions и дополнительные native сведения | Точный перенос всех видов запретов и constraintRegions ещё не подтверждён |

Локальный SDK также описывает `getState_Model3D()` с необязательным именем связанной библиотечной 3D-модели. Это имя можно рассматривать как дополнительную подсказку адаптера при распознавании корпуса; оно само по себе не даёт размеров. Отдельное поле для него в текущий эскиз не добавлено, CAD 3D-геометрия не импортируется.

**Расширение формы небольшое, но работа касается не только зон.** Зоны и корпуса — главные добавления; ещё потребуется надёжно заполнить существующие поля. Геометрические проверки используют медь и связность; DC требует толщин, проводимости, металлизации и нагрузок; SI — stackup/электрических материалов и моделей канала; thermal — тепловых материалов, мощностей и условий охлаждения. Один только импорт зон не обеспечивает все эти расчёты.

## Как читать заполнение

`zone.outline` — контур исходного объекта. **`zone.filledPolygons` — реально существующая медь CAD**: островки, вырезы, thermals и hatch уже отражены в геометрии. Для Fill контур тоже не считается отдельным слоем меди поверх результата. В каждой зоне хранится одна запись на каждый слой `layers`; при `available` массив `polygons: []` означает достоверно пустой результат.

Нельзя заменять failed/missing на пустой массив. Для доступной меди freshness может оставаться unknown: это не запрещает показывать или исследовать её, но расчёт должен сообщить об этом. Для постоянного Fill current означает соответствие прочитанному состоянию объекта, а не наличие операции refill.

Параметры `priority`, `clearanceMm`, `fill`, `padConnection` и другие существующие поля Pour используются для возможного восстановления. `rules.refill` добавляет недостающие различия правил. Этот TS-эскиз не закрывает все advanced режимы из аудита: неизвестный tie-break приоритетов, overrides, способ удаления островков и другие пробелы перечисляются в `unresolved`, поэтому такой набор не становится `complete` автоматически.

Свой refill вызывается для Pour, когда это требуется сценарием и хватает данных. Его результат хранится в результате запуска анализа, с `source: 'refill'`, zone ID и параметрами движка; исходные `source: 'cad'` полигоны сохраняются. На один расчёт выбирается одна версия. Физически дефектная CAD-медь сначала проверяется как прочитана.

## Дуги и RawPcb после уточнения

- Линия/дуга дорожки → `points[]` с одной шириной; круг/дуга сложного контура → точки `PolygonMm`. Отдельных массивов `arcs` и curve-сегментов нет.
- Предлагаемая настройка адаптера — максимальное отклонение границы в mm. Можно дополнительно ограничивать длину сегмента; фиксированная длина шага сама по себе не задаёт одинаковую ошибку для разных радиусов. Конкретный допуск ещё обсуждается.
- Адаптер должен контролировать отверстия и контакты. Если лимита точек не хватает, он сообщает об этом; увеличенный obstacle contour V1 не используется как точная медь.
- **Целевая общая модель — расширенный RoutingBoard.** В перспективе PCB Preview и остальные потребители RawPcb переводятся на неё, затем RawPcb удаляется. Native-чтение Pour/Poured/Fill сохраняется внутри экспортёра/адаптера.
- Перед удалением проверить потребителей: геометрия, rotation/side, hole offsets, bbox и отображение меди. Сейчас контракты ещё не полностью дублируются. Исходные CAD-команды при необходимости могут остаться внутренними данными адаптера, без второго публичного board-контракта.

Рабочий контракт предварительно зафиксирован для продолжения планирования и будет пересмотрен вместе со всем планом перед реализацией. Существующий RoutingBoard router, RawPcb и Preview в этой работе не меняются.
