# Independent bounded routing probe

Every board uses a job plan frozen on its legacy placement. A cutoff is an inconclusive bounded search, not proof that a connection is impossible. These are sampled jobs, not a routed PCB. Weights include net priority and fallback estimates, so compare penalties only within one board and its common job plan.

| Board | Mode | Found / jobs | Budget cutoffs | No path | Weighted penalty |
|---|---|---:|---:|---:|---:|
| Telemetry | legacy | 31 / 32 | 1 | 0 | 7835.99 |
| Telemetry | full-micro-single | 30 / 32 | 2 | 0 | 8359.43 |
| Telemetry | full-micro | 30 / 32 | 2 | 0 | 8359.43 |
| Telemetry | full-micro-repack | 30 / 32 | 2 | 0 | 8359.43 |
| Telemetry | full-off | 30 / 32 | 2 | 0 | 9793.9 |
| Telemetry | full-geometric-single | 31 / 32 | 1 | 0 | 9360.97 |
| Telemetry | full-geometric | 31 / 32 | 1 | 0 | 9356.97 |
| Telemetry | full-geometric-repack | 31 / 32 | 1 | 0 | 9356.97 |
| ESPower | legacy | 24 / 27 | 3 | 0 | 1781.81 |
| ESPower | full-micro-single | 24 / 27 | 3 | 0 | 6130.82 |
| ESPower | full-micro | 25 / 27 | 2 | 0 | 6033.58 |
| ESPower | full-micro-repack | 24 / 27 | 3 | 0 | 7513.99 |
| ESPower | full-off | 24 / 27 | 3 | 0 | 6353.44 |
| ESPower | full-geometric-single | 24 / 27 | 3 | 0 | 6346.85 |
| ESPower | full-geometric | 24 / 27 | 3 | 0 | 6348.43 |
| ESPower | full-geometric-repack | 24 / 27 | 3 | 0 | 6348.43 |
| esp32c3 | legacy | 12 / 22 | 10 | 0 | 2787.53 |
| esp32c3 | full-micro-single | 12 / 22 | 9 | 1 | 5920.86 |
| esp32c3 | full-micro | 12 / 22 | 9 | 1 | 5920.86 |
| esp32c3 | full-micro-repack | 12 / 22 | 9 | 1 | 5920.86 |
| esp32c3 | full-off | 12 / 22 | 9 | 1 | 5920.86 |
| esp32c3 | full-geometric-single | 12 / 22 | 9 | 1 | 5920.86 |
| esp32c3 | full-geometric | 12 / 22 | 9 | 1 | 5920.86 |
| esp32c3 | full-geometric-repack | 11 / 22 | 10 | 1 | 4297.8 |
