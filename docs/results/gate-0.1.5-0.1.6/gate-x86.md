**Instruction counts** (cachegrind, exact) against `d493d20a`; the gate fails above +1%.

| scene | base | head | change |
| --- | ---: | ---: | ---: |
| heavy-both | 1,094.3 M | 1,094.0 M | -0.02% |
| heavy-stream | 1,043.5 M | 1,043.2 M | -0.03% |
| heavy-scroll | 76.2 M | 76.3 M | +0.04% |
| heavy-stream-at | 1,059.5 M | 1,059.3 M | -0.02% |
| lab-stream | 461.3 M | 461.3 M | +0.02% |
| lab-scroll | 67.2 M | 67.2 M | -0.01% |
| animate | 293.0 M | 293.0 M | +0.00% |
| micro-state | 56.9 M | 57.0 M | +0.09% |
| micro-vscroll-fixed | 46.3 M | 46.3 M | -0.03% |
| micro-vscroll-variable | 84.1 M | 84.1 M | -0.01% |
| micro-state-luau | 79.7 M | 77.4 M | -3.00% (median of -2.886%, -3.243%, -2.999% under 3 heap placements) |
| micro-vscroll-fixed-luau | 63.4 M | 61.2 M | -3.46% (median of -3.460%, -3.457%, -3.484% under 3 heap placements) |
| micro-vscroll-variable-luau | 128.1 M | 125.7 M | -1.79% (median of -1.836%, -1.789%, -1.775% under 3 heap placements) |
| mount | 307.3 M | 302.5 M | -1.68% (median of -1.563%, -1.685%, -2.260% under 3 heap placements) |
| script-update | 1,447.7 M | 1,434.7 M | -0.89% (median of -0.892%, -0.854%, -0.886% under 3 heap placements) |
| anime | 3,728.5 M | 3,725.8 M | -0.07% (median of -0.072%, -0.094%, -0.047% under 3 heap placements) |
| script-layout | 3,020.7 M | 3,018.3 M | -0.08% (median of -0.082%, -0.085%, -0.085% under 3 heap placements) |
| list-state | 890.2 M | 887.1 M | -0.32% (median of -0.352%, -0.252%, -0.318% under 3 heap placements) |
| style | 1,327.8 M | 1,327.8 M | +0.00% |
| widgets | 147.8 M | 147.8 M | -0.00% |
| app-mail | 311.7 M | 311.4 M | -0.08% (median of -0.081%, -0.110%, -0.056% under 3 heap placements) |
| app-chat | 49.7 M | 48.3 M | -2.80% (±0.2% with no change) (median of -2.815%, -2.768%, -2.803% under 3 heap placements) |
| app-dashboard | 179.8 M | 177.2 M | -1.46% (median of -1.458%, -1.453%, -1.500% under 3 heap placements) |
| app-dashboard-sort | 120.2 M | 117.9 M | -1.98% (median of -1.932%, -2.008%, -1.975% under 3 heap placements) |
| app-sketch | 47.1 M | 45.7 M | -3.13% (median of -3.126%, -3.113%, -3.341% under 3 heap placements) |
| app-code | 100.3 M | 97.9 M | -2.33% (median of -2.326%, -2.303%, -2.355% under 3 heap placements) |
| app-code-search | 88.2 M | 86.0 M | -2.56% (median of -2.561%, -2.557%, -2.614% under 3 heap placements) |

<details><summary>Cells that grew, by whose code (instructions, head − base; functions in the job log and its artifact)</summary>

| scene | total | zgx | luau | libc | rust | linker | other |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| micro-state | +47,455 | +226 | +0 | +53,358 | -6,129 | +0 | +0 |
| heavy-scroll | +30,342 | -11,964 | +0 | +43,183 | -877 | +0 | +0 |
| lab-stream | +90,376 | +887 | +0 | +90,564 | -1,075 | +0 | +0 |

</details>

