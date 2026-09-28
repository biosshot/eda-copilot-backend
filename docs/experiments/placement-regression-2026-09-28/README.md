# Full-board placement regression, 2026-09-28

The [comparison](comparison.html) uses the same saved resolved input for both revisions of every board. `before` is placement code at `b8fe403`; `after` is placement code at `8311f7b`. The report includes the full board and every nonempty block and module, with both final board positions and saved local block layouts where available. Each run also exports an EasyEDA assembly JSON.

Status: 14/15 pairs completed. ThunderF722 was stopped when the investigation
shifted to runtime. Two fixtures have pre-existing hard violations on both sides;
the HTML identifies them. This is a partial regression report, not a claim that
every fixture passed. Signal paths were initially hidden by the report renderer;
they are now shown, with soft-limit failures listed separately. Saved solver
results and their original timings were not changed by re-rendering.

The input bank is `../global-placement-2026-09-27/`. It contains 15 distinct usable boards after deduplicating the ESPower and esp32c3 captures. Seven older fixtures have no usable saved full-board input; their individual reasons are listed in the HTML report. The runner reads the same `input.json` for both revisions and records its SHA-256. Both revisions use `PCB_LAYOUT_SUBTREE_WORKERS=0`, `PCB_POST_PLACE_THREADS=4`, and `PCB_BOARD_PACKER_THREADS=4`.

Runtime in each `summary.json` measures the full `autoPlacePcbWithReportAsync` call plus assembly export and metric calculation. It excludes SVG and HTML rendering. Runs share one host and some execute concurrently, so the values describe this batch's cost rather than isolated performance. Timed-out attempts are excluded; only completed runs appear in the before/after totals.

The automated verification checks placement inventory, unchanged fixed positions, assembly inventory and schema, input hashes, and existence of every referenced SVG. Electrical comparisons show total MST length, crossings of ratsnest lines, crossings of foreign pads, and individual nets that exceed a conservative length tolerance. These are placement diagnostics, not a routing result.

Rebuild the report from saved run results with:

```powershell
node --import tsx scripts/experiment-placement-regression-report.mjs
node --import tsx scripts/experiment-placement-regression-verify.mjs
```

To rerun a board against both revisions, use `node scripts/experiment-placement-regression-batch.mjs BOARD_NAME`. The batch skips completed results and refuses to append results from another revision. The baseline checkout is the sibling Git worktree `eda-copilot-baseline-run` at `b8fe403` and contains a copy of `scripts/experiment-placement-regression-run.mjs`. An after checkout at `8311f7b` is required to complete the historical comparison; the working branch has since moved on.
