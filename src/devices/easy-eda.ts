import { type Component } from "#types/component.ts";
import { extractPinsFromComponent, getSymbol } from "./symbols/symbol-parser.ts";
import { fetchWithRetry } from "#utils/fetch-with-retry.ts";
import type { EasyEdaProductApiResponce, EasyEdaProduct, EasyEdaDeviceInfo, EasyEdaDeviceApiResult, EasyEdaApiResponce, SymbolInfo } from "#types/easy-eda-api.ts";
import { memoize } from "#utils/memoize.ts";
import { normalizeEasyEdaSymbolInfo } from "./easy-eda-datastr.ts";
import { getPartLibraryUuid, getPartUuid, type PartUuid } from "#types/lcsc.ts";

export const getEasyEdaSymbolInfo = memoize(async (symUuid: string, libraryUuid: string = 'lcsc') => {
    const res = await fetchWithRetry(`https://pro.easyeda.com/api/v2/components/${encodeURIComponent(symUuid)}?uuid=${encodeURIComponent(symUuid)}&path=${encodeURIComponent(libraryUuid)}`);
    if (!res.ok) {
        throw new Error(`HTTP error! status: ${res.status}`);
    }
    const json = await res.json() as EasyEdaApiResponce<SymbolInfo>;
    if (!json.success || !json.result) {
        throw new Error(`Not success: ${json.success} ${json.msg}`);
    }
    return normalizeEasyEdaSymbolInfo(json.result);
});

export const getEasyEdaDevice = memoize(async (partUuid: PartUuid) => {
    const uuid = getPartUuid(partUuid);
    const libraryUuid = getPartLibraryUuid(partUuid);
    const res = await fetchWithRetry(`https://pro.easyeda.com/api/devices/${encodeURIComponent(uuid)}?uuid=${encodeURIComponent(uuid)}&path=${encodeURIComponent(libraryUuid)}`);
    if (!res.ok) {
        throw new Error(`HTTP error! status: ${res.status}`);
    }
    const json = await res.json() as EasyEdaApiResponce<EasyEdaDeviceApiResult>;
    if (!json.success || !json.result) {
        throw new Error(`Not success: ${json.success} ${json.msg}`);
    }
    return json.result;
});

export function easyEdaProductToComponent(product: EasyEdaProduct): Component {
    const pins = extractPinsFromComponent(product);
    return {
        pins: pins ?? [],
        price: Number(product.price?.[0]?.[1]) || 0, // Fallback to 0 if price missing
        name: product.device_info?.attributes?.["Manufacturer Part"] || 'Unknown',
        manufacturer: product.manufacturer || 'Unknown',
        description: product.device_info?.description || '',
        part_uuid: product.device_info?.uuid ?? '',
        datasheet: product.device_info?.attributes?.Datasheet || '',
        designatorPattern: product.device_info?.attributes?.Designator ?? null,
        footprintName: product.device_info?.footprint_info.title ?? null
    };
}

export async function normalizeEasyEdaProductSymbolInfo(product: EasyEdaProduct) {
    const symbolInfo = product.device_info?.symbol_info;
    if (!symbolInfo) return product;

    product.device_info!.symbol_info = await normalizeEasyEdaSymbolInfo(symbolInfo);
    return product;
}

export async function easyEdaDeviceToComponent(device: EasyEdaDeviceInfo | EasyEdaDeviceApiResult): Promise<Component | undefined> {
    if (!('symbol' in device)) return easyEdaSearch(device.product_code).then(r => r[0]);
    const libraryUuid = device.owner?.uuid === '0819f05c4eef4c71ace90d822a990e87' ? 'lcsc' : device.owner?.uuid;
    const partUuid: PartUuid = libraryUuid && libraryUuid !== 'lcsc'
        ? { uuid: device.uuid, libraryUuid }
        : device.uuid;
    return easyEdaDeviceToComponentInLibrary(device, partUuid);
}

export async function easyEdaDeviceToComponentInLibrary(
    device: EasyEdaDeviceApiResult,
    partUuid: PartUuid,
): Promise<Component | undefined> {
    if (!device.symbol?.uuid || !device.footprint?.uuid) return undefined;
    const symbolInfo = await getEasyEdaSymbolInfo(device.symbol.uuid, getPartLibraryUuid(partUuid));
    const pins = extractPinsFromComponent({ device_info: { symbol_info: symbolInfo } } as EasyEdaProduct);
    return {
        pins: pins ?? [],
        price: 0,
        name: device.attributes?.["Manufacturer Part"] || device.display_title || device.title || 'Unknown',
        manufacturer: device.attributes?.Manufacturer || device.owner?.nickname || 'Unknown',
        description: device.description || '',
        part_uuid: partUuid,
        datasheet: device.attributes?.Datasheet || null,
        designatorPattern: device.attributes?.Designator ?? null,
        footprintName: device.footprint?.display_title ?? device.footprint?.title ?? null,
    };
}

export function canonicalEasyEdaPartUuid(device: EasyEdaDeviceApiResult, partUuid: PartUuid): PartUuid {
    if (typeof partUuid === 'string') return partUuid;
    if (partUuid.libraryUuid === 'lcsc') return partUuid.uuid;
    if (partUuid.libraryUuid === 'user' && device.owner?.uuid) {
        return { uuid: partUuid.uuid, libraryUuid: device.owner.uuid };
    }
    return partUuid;
}

type DeviceSearchResponse = EasyEdaApiResponce<{
    lists: Record<string, EasyEdaDeviceApiResult[]>;
    page: number | string;
    pageSize: number | string;
    totalPage: number;
    count: number;
}>;

export async function easyEdaDeviceSearch(query: string, libraryUuid: string, page = 1, limit = 10) {
    const body = new URLSearchParams({
        uid: libraryUuid,
        path: libraryUuid,
        wd: query,
        page: String(page),
        pageSize: String(limit),
        withSymbolPackage: 'true',
    });
    const response = await fetchWithRetry('https://pro.easyeda.com/api/devices/search', { method: 'POST', body }) as unknown as Response;
    if (!response.ok) throw new Error(`EasyEDA device search failed: ${response.status}`);
    const json = await response.json() as DeviceSearchResponse;
    if (!json.success || !json.result) throw new Error(`EasyEDA device search failed: ${json.msg ?? 'unknown error'}`);
    const devices = json.result.lists[libraryUuid] ?? [];
    const components = (await Promise.all(devices.map(async device => {
        const requestedPartUuid: PartUuid = libraryUuid === 'lcsc' ? device.uuid : { uuid: device.uuid, libraryUuid };
        const partUuid = canonicalEasyEdaPartUuid(device, requestedPartUuid);
        return easyEdaDeviceToComponentInLibrary(device, partUuid).catch(() => undefined);
    }))).filter((component): component is Component => Boolean(component));
    return {
        components,
        page: Number(json.result.page),
        pageSize: Number(json.result.pageSize),
        totalPage: json.result.totalPage,
        count: json.result.count,
    };
}

export async function easyEdaSearch(
    data: { catalogId: number, params: { [key: string]: string | string[] } | null, currPage: number, pageSize: number } | string,
    filter: (comp: Component) => boolean = () => true,
    fullPage = true
): Promise<Component[]> {
    const MAX_PER_PAGE = 50; // API enforces maximum 50 items per page
    const pathParam = '0819f05c4eef4c71ace90d822a990e87'; // Fixed path for keyword search
    let targetPageSize: number;
    let startingPage: number;
    let urlParamsForCatalog: string | null = null;

    // Determine search mode and initialize parameters
    if (typeof data === "string") {
        targetPageSize = 5; // Default to max page size for keyword search
        startingPage = 1;
    } else {
        targetPageSize = data.pageSize;
        startingPage = Math.max(1, data.currPage); // Ensure valid starting page
        // Precompute catalog parameters to avoid recalculating in loop
        urlParamsForCatalog = Object.entries(data.params ?? {})
            .map(([key, value]) => (Array.isArray(value) ? value : [value])
                .map(v => `${v}@${key}`)
                .join('$'))
            .join('$');
    }

    const collected: EasyEdaProduct[] = []; // Stores raw component data before mapping
    let result: Component[] = []; // Stores raw component data before mapping

    let currentPage = startingPage;

    // Continue fetching until we have enough components or run out of pages
    while (result.length < targetPageSize) {
        let response: Response;
        let json: EasyEdaProductApiResponce;

        try {
            // Handle keyword search (POST request)
            if (typeof data === "string") {
                const body = new URLSearchParams({
                    keyword: data,
                    currPage: currentPage.toString(),
                    pageSize: MAX_PER_PAGE.toString(),
                    path: pathParam
                });
                response = await fetchWithRetry('https://pro.easyeda.com/api/v2/eda/product/search', {
                    method: 'POST',
                    body
                }) as unknown as Response;;
            }
            // Handle catalog search (GET request)
            else {
                const apiUrl = `https://pro.easyeda.com/api/v2/eda/product/list?` +
                    `catalog=${encodeURIComponent(data.catalogId)}` +
                    `&currPage=${currentPage}` +
                    `&pageSize=${MAX_PER_PAGE}` +
                    `&param=${encodeURIComponent(urlParamsForCatalog!)}`;
                response = await fetchWithRetry(apiUrl) as unknown as Response;
            }

            json = await response.json();
        } catch (error) {
            throw new Error(`Network error on page ${currentPage}: ${error instanceof Error ? error.message : 'Unknown error'}`);
        }

        // Validate API response
        if (json.code !== 200 && json.code !== 0 || !json.result) {
            throw new Error(`API error (page ${currentPage}): Code ${json.code}, Message: ${json.msg}`);
        }

        // Filter valid components (with symbol data and datasheet)
        const productsWithDatasheet = json.result.productList.filter(element =>
            (element?.device_info?.symbol_info?.dataStr || element?.device_info?.symbol_info?.dataStrId) &&
            element?.device_info?.attributes?.Datasheet
        );

        const validComponents = (await Promise.all(productsWithDatasheet.map(async element => {
            try {
                return await normalizeEasyEdaProductSymbolInfo(element);
            } catch {
                return null;
            }
        }))).filter((element): element is EasyEdaProduct =>
            Boolean(element?.device_info?.symbol_info?.dataStr)
        );

        // Add needed components without exceeding target page size
        const remainingSlots = targetPageSize - collected.length;
        collected.push(...validComponents.slice(0, remainingSlots));

        // Determine if more pages exist
        const pageInfo = json.result.pageInfo;
        const isLastPage = pageInfo
            ? currentPage >= pageInfo.totalPage // Use API-provided pagination
            : json.result.productList.length < MAX_PER_PAGE; // Fallback for missing pageInfo


        result = collected.map(easyEdaProductToComponent).filter(filter);

        // Termination conditions
        if (result.length >= targetPageSize || isLastPage || fullPage) {
            break;
        }

        currentPage++;
    }

    // Map raw components to final Component structure
    return result;
}

export async function easyEdaSearcPassiveComponent({ componentType = '', value = '', currPage = 1, pageSize = 10 }) {
    const catalogIdMap: { [key: string]: number } = {
        'capacitor': 312,
        'inductor': 316,
        'resistor': 308,
        'connector': 644,

        "Конденсаторы": 312,
        "Резисторы": 308,
        "Индуктивности": 316,
        "Разъемы": 644
    };
    const catalogId = catalogIdMap[componentType];

    const params: { [key: string]: string[] } = {};

    if (catalogId === 312) {
        params["Capacitance"] = [value + "F"];
    } else if (catalogId === 316) {
        params["Inductance"] = [value + "H"];
    } else if (catalogId === 308) {
        params["Resistance"] = [value + 'Ω'];
    } else if (catalogId === 644) {
        params["Number of Pins"] = [value + 'P'];
    }

    return await easyEdaSearch({ catalogId, params, currPage: currPage ?? 1, pageSize: pageSize ?? 25 });
}
