import { copyFile, readFile, writeFile } from 'node:fs/promises';

await copyFile(new URL('../src/pcb-layout/pcb-layout-dsl/spec-doc.d.ts', import.meta.url),
  new URL('../dist/spec-doc.d.ts', import.meta.url));
// Source declarations share the actual V2 types. Installed standalone docs refer
// to the same public types entry, without exposing a source-tree-relative import.
const analysisDoc = await readFile(new URL('../src/analyze/analysis-dsl/spec-doc.d.ts', import.meta.url), 'utf8');
await writeFile(new URL('../dist/analysis-spec-doc.d.ts', import.meta.url),
  analysisDoc.replace('../../types/pcb/routing-board-v2.ts', 'eda-copilot-backend/types'));
