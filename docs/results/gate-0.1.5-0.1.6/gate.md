**Instruction counts** (cachegrind, exact) against `d493d20a`; the gate fails above +100%.

| scene | base | head | change |
| --- | ---: | ---: | ---: |
| heavy-both | 998.4 M | 998.1 M | -0.03% |
| heavy-stream | 950.3 M | 950.0 M | -0.03% |
| heavy-scroll | 75.2 M | 75.2 M | -0.02% |
| heavy-stream-at | 965.7 M | 965.4 M | -0.03% |
| lab-stream | 431.4 M | 431.4 M | -0.00% |
| lab-scroll | 66.1 M | 66.1 M | -0.02% |
| animate | 276.2 M | 276.2 M | -0.00% |
| micro-state | 56.2 M | 56.2 M | +0.00% |
| micro-vscroll-fixed | 45.9 M | 45.9 M | -0.02% |
| micro-vscroll-variable | 82.0 M | 82.0 M | -0.04% |
| micro-state-luau | 79.5 M | 77.1 M | -2.98% (median of -2.970%, -3.049%, -2.978% under 3 heap placements) |
| micro-vscroll-fixed-luau | 62.9 M | 60.7 M | -3.56% (median of -3.559%, -3.552%, -3.559% under 3 heap placements) |
| micro-vscroll-variable-luau | 126.7 M | 124.3 M | -1.95% (median of -1.951%, -1.952%, -1.959% under 3 heap placements) |
| mount | 302.2 M | 297.1 M | -1.95% (median of -1.690%, -1.951%, -1.961% under 3 heap placements) |
| script-update | 1,416.6 M | 1,404.4 M | -0.86% (median of -0.861%, -0.857%, -0.867% under 3 heap placements) |
| anime | 2,949.7 M | 2,948.1 M | -0.06% (median of -0.056%, -0.096%, -0.047% under 3 heap placements) |
| script-layout | 2,820.3 M | 2,817.5 M | -0.10% (median of -0.096%, -0.116%, -0.091% under 3 heap placements) |
| list-state | 822.8 M | 819.5 M | -0.40% (median of -0.403%, -0.316%, -0.465% under 3 heap placements) |
| style | 1,246.7 M | 1,246.7 M | -0.00% |
| widgets | 137.9 M | 137.9 M | +0.00% |
| app-mail | 286.5 M | 286.0 M | -0.12% (median of -0.158%, -0.118%, -0.114% under 3 heap placements) |
| app-chat | 47.8 M | 46.4 M | -2.90% (±0.2% with no change) (median of -2.934%, -2.893%, -2.898% under 3 heap placements) |
| app-dashboard | 169.3 M | 166.9 M | -1.46% (median of -1.458%, -1.455%, -1.477% under 3 heap placements) |
| app-dashboard-sort | 112.5 M | 110.2 M | -2.07% (median of -2.068%, -2.068%, -2.100% under 3 heap placements) |
| app-sketch | 45.7 M | 44.3 M | -3.10% (median of -3.097%, -3.089%, -3.132% under 3 heap placements) |
| app-code | 94.3 M | 91.7 M | -2.75% (median of -2.751%, -2.752%, -2.719% under 3 heap placements) |
| app-code-search | 83.3 M | 80.8 M | -3.05% (median of -3.051%, -3.050%, -3.009% under 3 heap placements) |
