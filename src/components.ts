import { z } from 'zod';
import { canonicalEasyEdaPartUuid, easyEdaDeviceSearch, easyEdaDeviceToComponentInLibrary, easyEdaSearch, getEasyEdaDevice, getEasyEdaSymbolInfo } from './devices/easy-eda.ts';
import { getPartLibraryUuid, PartUuidStruct } from './types/lcsc.ts';

export const componentLibraries = [
  { libraryUuid: 'lcsc', name: 'EasyEDA Pro System / LCSC', search: true, resolve: true },
  { libraryUuid: 'user', name: 'EasyEDA Pro Public', search: true, resolve: true },
] as const;

export function libraryList() {
  return { libraries: componentLibraries, acceptsExplicitLibraryUuid: true };
}

const inputSchema = z.object({
  MPN: z.string().min(1).nullish(),
  library_uuid: z.string().min(1).default('lcsc'),
  part_uuid: PartUuidStruct().nullish(),
}).refine(input => Boolean(input.MPN || input.part_uuid), 'Fill one: MPN or part_uuid');

export type ComponentSearchInput = z.input<typeof inputSchema>;

/** Raw library symbol for read-only previews. Includes every PART section. */
export async function componentSymbol(partUuid: z.infer<ReturnType<typeof PartUuidStruct>>) {
  const device = await getEasyEdaDevice(partUuid);
  if (!device.symbol?.uuid) throw new Error('Component has no symbol.');
  const canonicalPartUuid = canonicalEasyEdaPartUuid(device, partUuid);
  const symbol = await getEasyEdaSymbolInfo(device.symbol.uuid, getPartLibraryUuid(canonicalPartUuid));
  if (!symbol.dataStr) throw new Error('Component symbol is unavailable.');
  return { dataStr: symbol.dataStr };
}

export async function componentSearch(input: ComponentSearchInput) {
  const data = inputSchema.parse(input);
  if (data.part_uuid) {
    const device = await getEasyEdaDevice(data.part_uuid).catch(() => undefined);
    const partUuid = device && canonicalEasyEdaPartUuid(device, data.part_uuid);
    const result = device && partUuid && await easyEdaDeviceToComponentInLibrary(device, partUuid).catch(() => undefined);
    if (!result) throw new Error('Component not found.');
    return { bestComponent: result };
  }
  if (data.library_uuid !== 'lcsc') {
    return { ...(await easyEdaDeviceSearch(data.MPN!, data.library_uuid)), bestComponent: null };
  }
  return { components: (await easyEdaSearch(data.MPN!)).slice(0, 10), bestComponent: null };
}

/** Reusable block discovery remains disabled, matching the current MCP service. */
export async function searchReusedBlock(_input?: { query: string; page?: number; limit?: number }): Promise<never[]> {
  return [];
}

export type { Component } from './types/component.ts';
