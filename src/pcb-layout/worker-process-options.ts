import { createRequire } from "node:module";
import type { ForkOptions } from "node:child_process";

// workerpool 10 validates fork options but omits Node's windowsHide option.
// Extend only that allowlist; keep its validation and process isolation intact.
export const placementWorkerForkOptions: ForkOptions & { windowsHide?: boolean } = {};

if (process.platform === "win32") {
    try {
        const require = createRequire(import.meta.url);
        const { forkOptsNames } = require("workerpool/src/validateOptions.js") as { forkOptsNames: string[] };
        if (!forkOptsNames.includes("windowsHide")) forkOptsNames.push("windowsHide");
        placementWorkerForkOptions.windowsHide = true;
    } catch {
        // If workerpool's internals change, keep placement working with default
        // fork options instead of passing an option it may reject.
    }
}
