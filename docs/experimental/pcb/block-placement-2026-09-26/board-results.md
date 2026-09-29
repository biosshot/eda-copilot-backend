# Full-board exploratory results

Timing is single-run wall time under variable concurrent load; it is not a performance acceptance benchmark. Lengths are mm. HPWL and pairSum are different metrics and must not be added. Placement ok is not routing completeness.

| Fixture | Variant | Placement ok | HPWL | Two-terminal sum | Worst pair | Fixed changes | Seconds |
|---|---|---|---:|---:|---:|---:|---:|
| Telemetry | ALL | true | 2985.5 | 947.72 | 66.88 | 0 | 60.11 |
| Telemetry | ALLP | true | 3007.61 | 882.17 | 80.51 | 0 | 55.22 |
| Telemetry | ALLR | true | 2957.25 | 795.11 | 69.12 | 0 | 60.16 |
| Telemetry | ALLX | true | 2905.82 | 932.66 | 69.34 | 0 | 57.58 |
| Telemetry | B0 | true | 2716.39 | 781.84 | 63.08 | 0 | 60.67 |
| Telemetry | L | true | 2734.6 | 782.26 | 64.54 | 0 | 54.73 |
| Telemetry | N | true | 2753.24 | 790.97 | 60.64 | 0 | 51.99 |
| Telemetry | NCL | true | 2751.35 | 796.41 | 60.53 | 0 | 60.34 |
| Telemetry | NCLP | true | 2752.91 | 796.71 | 60.53 | 0 | 63.22 |
| Telemetry | NCLR | true | 2746.14 | 791.19 | 61.02 | 0 | 58.49 |
| Telemetry | NCLW | true | 2780.69 | 816.35 | 66.88 | 0 | 52.66 |
| Telemetry | NCLX | true | 2749.89 | 800.45 | 60.53 | 0 | 58.36 |
| Telemetry | NWSC | true | 3004.17 | 947.57 | 69.34 | 0 | 53.74 |
| ESPower | ALL | true | 458.34 | 98.01 | 42.33 | 0 | 13.56 |
| ESPower | ALLP | true | 460.01 | 96.77 | 42.33 | 0 | 13.28 |
| ESPower | ALLR | true | 431.5 | 89.13 | 42.33 | 0 | 15.18 |
| ESPower | ALLX | true | 460.26 | 99.2 | 42.33 | 0 | 10.29 |
| ESPower | B0 | true | 435.51 | 95.23 | 42.33 | 0 | 8.09 |
| ESPower | L | true | 435.51 | 95.23 | 42.33 | 0 | 10.61 |
| ESPower | N | true | 437.43 | 92.43 | 42.33 | 0 | 11.98 |
| ESPower | NCL | true | 437.19 | 94.77 | 42.33 | 0 | 8.5 |
| ESPower | NCLP | true | 437.23 | 99.45 | 42.33 | 0 | 9.04 |
| ESPower | NCLR | true | 443.7 | 98.47 | 42.33 | 0 | 12.55 |
| ESPower | NCLW | true | 440.79 | 95.78 | 42.33 | 0 | 13.24 |
| ESPower | NCLX | true | 437.19 | 94.77 | 42.33 | 0 | 8.52 |
| ESPower | NWSC | true | 458.08 | 99.96 | 42.33 | 0 | 12.75 |
| esp32c3 | ALL | true | 581.26 | 304.31 | 30.93 | 0 | 7.55 |
| esp32c3 | ALLP | true | 581.26 | 304.31 | 30.93 | 0 | 7.06 |
| esp32c3 | ALLR | true | 581.66 | 307.74 | 30.93 | 0 | 6.5 |
| esp32c3 | ALLX | true | 581.26 | 304.31 | 30.93 | 0 | 6.26 |
| esp32c3 | B0 | true | 561.56 | 302.77 | 30.93 | 0 | 7.04 |
| esp32c3 | L | true | 561.56 | 302.77 | 30.93 | 0 | 7.38 |
| esp32c3 | N | true | 561.56 | 302.77 | 30.93 | 0 | 7.44 |
| esp32c3 | NCL | true | 563.21 | 302.77 | 30.93 | 0 | 6.04 |
| esp32c3 | NCLP | true | 563.21 | 302.77 | 30.93 | 0 | 5.92 |
| esp32c3 | NCLR | true | 564.49 | 307.44 | 30.93 | 0 | 7.17 |
| esp32c3 | NCLW | true | 581.26 | 304.31 | 30.93 | 0 | 7.41 |
| esp32c3 | NCLX | true | 563.21 | 302.77 | 30.93 | 0 | 5.94 |
| esp32c3 | NWSC | true | 581.26 | 304.31 | 30.93 | 0 | 7.46 |
