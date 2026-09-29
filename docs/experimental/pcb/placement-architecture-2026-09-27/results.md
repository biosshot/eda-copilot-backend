# Whole-board experiments

[Что означают режимы и как они переключаются — объяснение по-русски](modes-ru.md). Each row is a separate manually selected configuration, not an automatic competition between modes.

All modes use the ordinary tree solver. No injected native options. Subtree workers and native solve cache are disabled; postrefine uses one thread. Timings are single samples, not a statistical benchmark. Valid placement does not establish routability.

| Board | Mode | Valid | Pair sum, mm | Worst pair, mm | HPWL, mm | Block time, s | Total time, s | Accepted blocks |
|---|---|---|---:|---:|---:|---:|---:|---:|
| Telemetry | legacy | true | 781.84 | 63.08 | 2716.39 | 7.42 | 61.12 | 0 |
| Telemetry | full-micro-single | true | 821.43 | 69.12 | 2967.11 | 15.45 | 65.25 | 0 |
| Telemetry | full-micro | true | 821.22 | 69.12 | 2964.5 | 15.96 | 71.08 | 6 |
| Telemetry | full-micro-repack | true | 821.22 | 69.12 | 2964.5 | 16.92 | 146.1 | 6 |
| Telemetry | full-off | true | 820.48 | 68.96 | 2785.2 | 13.28 | 70.95 | 7 |
| Telemetry | full-geometric-single | true | 958.07 | 67.01 | 3072.34 | 13.16 | 64.7 | 0 |
| Telemetry | full-geometric | true | 949.02 | 67.01 | 3045.94 | 13.03 | 64.63 | 6 |
| Telemetry | full-geometric-repack | true | 949.02 | 67.01 | 3045.94 | 13.2 | 139.2 | 6 |
| ESPower | legacy | true | 95.23 | 42.33 | 435.51 | 3.31 | 8.19 | 0 |
| ESPower | full-micro-single | true | 94.01 | 42.33 | 438.2 | 9.13 | 14.59 | 0 |
| ESPower | full-micro | true | 93.52 | 42.33 | 436.76 | 11.57 | 17.89 | 2 |
| ESPower | full-micro-repack | true | 92.32 | 42.33 | 436.44 | 10.45 | 24.16 | 1 |
| ESPower | full-off | true | 94.12 | 42.33 | 446.04 | 9.06 | 15.62 | 2 |
| ESPower | full-geometric-single | true | 99.29 | 42.33 | 436.24 | 7.54 | 13.96 | 0 |
| ESPower | full-geometric | true | 100.85 | 42.33 | 441.55 | 8.04 | 14.15 | 2 |
| ESPower | full-geometric-repack | true | 100.85 | 42.33 | 441.55 | 8.45 | 23.47 | 2 |
| esp32c3 | legacy | true | 302.77 | 30.93 | 561.56 | 4.23 | 5.71 | 0 |
| esp32c3 | full-micro-single | true | 307.74 | 30.93 | 581.65 | 5.01 | 7.03 | 0 |
| esp32c3 | full-micro | true | 306.04 | 30.93 | 579.65 | 4.89 | 6.6 | 2 |
| esp32c3 | full-micro-repack | true | 306.04 | 30.93 | 579.65 | 5.05 | 6.99 | 2 |
| esp32c3 | full-off | true | 306.04 | 30.93 | 579.65 | 4.78 | 6.91 | 2 |
| esp32c3 | full-geometric-single | true | 307.77 | 30.93 | 581.7 | 4.68 | 6.52 | 0 |
| esp32c3 | full-geometric | true | 306.07 | 30.93 | 579.7 | 4.79 | 6.55 | 2 |
| esp32c3 | full-geometric-repack | true | 306.07 | 30.93 | 586.84 | 4.93 | 7.38 | 1 |
