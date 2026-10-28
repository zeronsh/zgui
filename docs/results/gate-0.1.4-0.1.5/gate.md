**Instruction counts** (cachegrind, exact) against `6dd8108bb`; the gate fails above +100%.

| scene | base | head | change |
| --- | ---: | ---: | ---: |
| heavy-both | 1,023.5 M | 1,016.6 M | -0.68% |
| heavy-stream | 972.5 M | 965.7 M | -0.69% |
| heavy-scroll | 76.0 M | 75.7 M | -0.36% |
| heavy-stream-at | 972.5 M | 965.7 M | -0.69% |
| lab-stream | 431.9 M | 431.9 M | -0.00% |
| lab-scroll | 66.2 M | 66.1 M | -0.17% |
| animate | 277.1 M | 276.2 M | -0.30% |
| micro-state | 56.6 M | 56.4 M | -0.31% |
| micro-vscroll-fixed | 46.0 M | 45.9 M | -0.25% |
| micro-vscroll-variable | 82.5 M | 82.0 M | -0.53% |
| micro-state-luau | 79.9 M | 79.7 M | -0.21% (median of -0.221%, -0.215%, -0.207% under 3 heap placements) |
| micro-vscroll-fixed-luau | 63.0 M | 62.9 M | -0.07% (median of -0.077%, -0.069%, -0.071% under 3 heap placements) |
| micro-vscroll-variable-luau | 127.1 M | 126.8 M | -0.28% (median of -0.278%, -0.271%, -0.297% under 3 heap placements) |
| mount | 303.8 M | 302.3 M | -0.51% (median of -0.505%, -0.455%, -0.592% under 3 heap placements) |
| script-update | 1,422.3 M | 1,421.2 M | -0.08% (median of -0.078%, -0.075%, -0.094% under 3 heap placements) |
| anime | 3,027.8 M | 2,949.8 M | -2.57% (median of -2.576%, -2.566%, -2.573% under 3 heap placements) |
| script-layout | 2,823.7 M | 2,820.1 M | -0.12% (median of -0.124%, -0.117%, -0.112% under 3 heap placements) |
| list-state | 824.6 M | 824.4 M | -0.02% (median of -0.016%, -0.089%, +0.157% under 3 heap placements; layout) |
| style | 1,245.4 M | 1,246.7 M | +0.10% |
| widgets | 138.2 M | 137.9 M | -0.25% |
| app-mail | 286.6 M | 286.6 M | +0.00% |
| app-chat | 47.8 M | 47.8 M | -0.06% (±0.2% with no change) (median of -0.061%, -0.055%, -0.071% under 3 heap placements) |
| app-dashboard | 170.3 M | 170.2 M | -0.04% (median of -0.038%, -0.035%, +0.039% under 3 heap placements; layout) |
| app-dashboard-sort | 113.5 M | 113.4 M | -0.06% (median of -0.064%, -0.060%, -0.030% under 3 heap placements) |
| app-sketch | 45.8 M | 45.8 M | -0.09% (median of -0.091%, -0.092%, -0.091% under 3 heap placements) |
| app-code | 93.1 M | 94.3 M | +1.31% (median of +1.313%, +1.314%, +1.355% under 3 heap placements) |
| app-code-search | 82.2 M | 83.3 M | +1.38% (median of +1.376%, +1.377%, +1.394% under 3 heap placements) |
