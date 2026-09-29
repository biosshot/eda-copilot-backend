# Circuit layout module

`index.ts` is the schematic placement entry point; `layout.ts` and related files handle arrangement; `patterns/` detects reusable circuit structures; `refinement/` improves positions and wires. Search the pattern registry, existing refinement passes, geometry, and signal helpers before adding a new pattern or utility. Preserve pin/net identities and readable routing while keeping changes local to the relevant pass.

Run `npm run typecheck` and focused circuit/pattern tests after edits. `npm run test:schematics -- --help` lists gallery, filter, offline, and worker options. Generated galleries and caches belong under `debugging/circuit-layout/`; retain only selected evidence in documentation.
