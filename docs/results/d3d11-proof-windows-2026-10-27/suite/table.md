
### heavy idle

| | zgx | zgx-d3d11 | zgx-d3d11-threaded | D3D11 | D3D11 threaded |
| --- | --- | --- | --- | --- | --- |
| app CPU, M cycles/s | 2.03 | 1.08 | 1.38 | -47% | -32% |
| app GPU, ms/s | 0 | 0 | 0 |  |  |
| DWM CPU, M cycles/s | 0.0436 | 0.292 | 0.919 | +570% | +2010% |
| DWM GPU, ms/s | 0 | 0.000229 | 0.00336 |  |  |
| private, MiB | 146 | 66.6 | 69.8 | -54% | -52% |
| working set, MiB | 114 | 55.5 | 58.9 | -51% | -48% |
| board, W | 7.37 | 7.32 | 7.64 | -1% | +4% |

### heavy stream

| | zgx | zgx-d3d11 | zgx-d3d11-threaded | D3D11 | D3D11 threaded |
| --- | --- | --- | --- | --- | --- |
| app CPU, M cycles/s | 279 | 237 | 283 | -15% | +1% |
| app CPU, M cycles/frame | 4.82 | 4.09 | 4.86 | -15% | +1% |
| app GPU, ms/s | 11 | 9.8 | 9.91 | -11% | -10% |
| DWM CPU, M cycles/s | 122 | 125 | 126 | +3% | +3% |
| DWM GPU, ms/s | 2.45 | 2.46 | 2.46 | +0% | +0% |
| private, MiB | 152 | 68.9 | 73.2 | -55% | -52% |
| working set, MiB | 121 | 59.7 | 63.3 | -50% | -48% |
| board, W | 14.6 | 14.5 | 14.8 | -1% | +1% |
| pacing p50, ms | 16.7 | 16.7 | 16.7 | | |
| pacing p99, ms | 17.4 | 17.3 | 17.3 | | |
| long intervals | 0.00873 | 0.00429 | 0 | | |

### heavy scroll

| | zgx | zgx-d3d11 | zgx-d3d11-threaded | D3D11 | D3D11 threaded |
| --- | --- | --- | --- | --- | --- |
| app CPU, M cycles/s | 123 | 86.4 | 118 | -30% | -4% |
| app CPU, M cycles/frame | 2.13 | 1.52 | 2.03 | -29% | -5% |
| app GPU, ms/s | 2.12 | 2.93 | 3.02 | +38% | +42% |
| DWM CPU, M cycles/s | 122 | 119 | 126 | -2% | +3% |
| DWM GPU, ms/s | 2.11 | 2.1 | 2.1 | -0% | -0% |
| private, MiB | 148 | 66.5 | 68.4 | -55% | -54% |
| working set, MiB | 117 | 56.5 | 59.5 | -52% | -49% |
| board, W | 14.6 | 14.4 | 14.4 | -1% | -1% |
| pacing p50, ms | 16.7 | 16.7 | 16.7 | | |
| pacing p99, ms | 17.2 | 17.3 | 17.2 | | |
| long intervals | 0.00421 | 0.00847 | 0 | | |

### heavy both

| | zgx | zgx-d3d11 | zgx-d3d11-threaded | D3D11 | D3D11 threaded |
| --- | --- | --- | --- | --- | --- |
| app CPU, M cycles/s | 329 | 267 | 324 | -19% | -1% |
| app CPU, M cycles/frame | 5.67 | 4.61 | 5.58 | -19% | -2% |
| app GPU, ms/s | 13.5 | 11.9 | 12 | -12% | -11% |
| DWM CPU, M cycles/s | 134 | 118 | 127 | -12% | -5% |
| DWM GPU, ms/s | 2.48 | 2.47 | 2.46 | -1% | -1% |
| private, MiB | 153 | 70.8 | 74.3 | -54% | -52% |
| working set, MiB | 122 | 60.7 | 64.2 | -50% | -47% |
| board, W | 14.1 | 14.5 | 14.7 | +3% | +4% |
| pacing p50, ms | 16.7 | 16.7 | 16.7 | | |
| pacing p99, ms | 17.3 | 17.3 | 33.2 | | |
| long intervals | 0.0021 | 0.0042 | 0.013 | | |

### lab stream

| | zgx | zgx-d3d11 | zgx-d3d11-threaded | D3D11 | D3D11 threaded |
| --- | --- | --- | --- | --- | --- |
| app CPU, M cycles/s | 93.7 | 63.1 | 90.9 | -33% | -3% |
| app CPU, M cycles/frame | 1.64 | 1.09 | 1.56 | -34% | -4% |
| app GPU, ms/s | 3.14 | 3.96 | 3.97 | +26% | +26% |
| DWM CPU, M cycles/s | 90.3 | 93.8 | 101 | +4% | +11% |
| DWM GPU, ms/s | 2.01 | 2.02 | 2.02 | +0% | +0% |
| private, MiB | 126 | 62 | 65.2 | -51% | -48% |
| working set, MiB | 96.6 | 52.9 | 55.7 | -45% | -42% |
| board, W | 15.3 | 14.7 | 14.4 | -4% | -6% |
| pacing p50, ms | 16.7 | 16.7 | 16.7 | | |
| pacing p99, ms | 17.4 | 17.3 | 17.2 | | |
| long intervals | 0.00636 | 0.00427 | 0.00424 | | |

### lab scroll

| | zgx | zgx-d3d11 | zgx-d3d11-threaded | D3D11 | D3D11 threaded |
| --- | --- | --- | --- | --- | --- |
| app CPU, M cycles/s | 88.6 | 56.6 | 102 | -36% | +15% |
| app CPU, M cycles/frame | 1.53 | 0.973 | 1.75 | -36% | +14% |
| app GPU, ms/s | 2.02 | 2.55 | 2.27 | +26% | +12% |
| DWM CPU, M cycles/s | 91.6 | 93.4 | 97.5 | +2% | +6% |
| DWM GPU, ms/s | 2.18 | 2.19 | 2.18 | +0% | -0% |
| private, MiB | 125 | 59.9 | 64.5 | -52% | -48% |
| working set, MiB | 95.4 | 51.4 | 54.2 | -46% | -43% |
| board, W | 15.1 | 14.6 | 15.1 | -4% | -0% |
| pacing p50, ms | 16.7 | 16.7 | 16.7 | | |
| pacing p99, ms | 17.3 | 17.3 | 17.3 | | |
| long intervals | 0.00425 | 0.00426 | 0.0063 | | |

### lab both

| | zgx | zgx-d3d11 | zgx-d3d11-threaded | D3D11 | D3D11 threaded |
| --- | --- | --- | --- | --- | --- |
| app CPU, M cycles/s | 111 | 75.2 | 114 | -32% | +2% |
| app CPU, M cycles/frame | 1.92 | 1.29 | 1.96 | -32% | +3% |
| app GPU, ms/s | 3.58 | 4.21 | 4.02 | +18% | +12% |
| DWM CPU, M cycles/s | 97 | 93.5 | 101 | -4% | +4% |
| DWM GPU, ms/s | 2.28 | 2.27 | 2.27 | -0% | -0% |
| private, MiB | 126 | 61.5 | 64.9 | -51% | -49% |
| working set, MiB | 96.9 | 52.7 | 55.7 | -46% | -43% |
| board, W | 15.4 | 15.1 | 15 | -2% | -3% |
| pacing p50, ms | 16.7 | 16.7 | 16.7 | | |
| pacing p99, ms | 17.3 | 17.3 | 17.2 | | |
| long intervals | 0.00423 | 0.00636 | 0.00419 | | |

### animate animate

| | zgx | zgx-d3d11 | zgx-d3d11-threaded | D3D11 | D3D11 threaded |
| --- | --- | --- | --- | --- | --- |
| app CPU, M cycles/s | 95 | 66.5 | 97 | -30% | +2% |
| app CPU, M cycles/frame | 1.61 | 1.12 | 1.64 | -30% | +2% |
| app GPU, ms/s | 2.05 | 2.69 | 2.68 | +31% | +30% |
| DWM CPU, M cycles/s | 98.9 | 98.4 | 105 | -1% | +6% |
| DWM GPU, ms/s | 2.01 | 2.01 | 2 | +0% | -1% |
| private, MiB | 125 | 60.1 | 63.7 | -52% | -49% |
| working set, MiB | 94.5 | 51.2 | 54.2 | -46% | -43% |
| board, W | 15.4 | 15.8 | 15.3 | +3% | -1% |
| pacing p50, ms | 16.7 | 16.7 | 16.7 | | |
| pacing p99, ms | 17.2 | 17.4 | 17.9 | | |
| long intervals | 0.00209 | 0.00211 | 0.00422 | | |

### vlist scroll

| | zgx | zgx-d3d11 | zgx-d3d11-threaded | D3D11 | D3D11 threaded |
| --- | --- | --- | --- | --- | --- |
| app CPU, M cycles/s | 92 | 60.9 | 81.1 | -34% | -12% |
| app CPU, M cycles/frame | 1.59 | 1.05 | 1.39 | -34% | -12% |
| app GPU, ms/s | 2.77 | 3.16 | 3.17 | +14% | +14% |
| DWM CPU, M cycles/s | 110 | 112 | 98.1 | +2% | -11% |
| DWM GPU, ms/s | 2.33 | 2.32 | 2.33 | -1% | -0% |
| private, MiB | 139 | 66.8 | 69.5 | -52% | -50% |
| working set, MiB | 108 | 56.7 | 59.8 | -48% | -45% |
| board, W | 15.4 | 14.8 | 14.8 | -4% | -4% |
| pacing p50, ms | 16.7 | 16.7 | 16.7 | | |
| pacing p99, ms | 17.3 | 17.3 | 17.4 | | |
| long intervals | 0.00425 | 0.00423 | 0.00844 | | |

### vlist fling

| | zgx | zgx-d3d11 | zgx-d3d11-threaded | D3D11 | D3D11 threaded |
| --- | --- | --- | --- | --- | --- |
| app CPU, M cycles/s | 92.2 | 69.3 | 93.8 | -25% | +2% |
| app CPU, M cycles/frame | 1.6 | 1.21 | 1.63 | -25% | +2% |
| app GPU, ms/s | 2.85 | 3.47 | 3.45 | +22% | +21% |
| DWM CPU, M cycles/s | 96.4 | 101 | 99.4 | +5% | +3% |
| DWM GPU, ms/s | 2.3 | 2.29 | 2.29 | -1% | -0% |
| private, MiB | 139 | 66.8 | 69.2 | -52% | -50% |
| working set, MiB | 109 | 56.9 | 59.5 | -48% | -45% |
| board, W | 15 | 14.8 | 14.5 | -2% | -4% |
| pacing p50, ms | 16.7 | 16.7 | 16.7 | | |
| pacing p99, ms | 33.1 | 33 | 31.5 | | |
| long intervals | 0.0171 | 0.0169 | 0.0127 | | |

### vlist jump

| | zgx | zgx-d3d11 | zgx-d3d11-threaded | D3D11 | D3D11 threaded |
| --- | --- | --- | --- | --- | --- |
| app CPU, M cycles/s | 91.7 | 66.1 | 95.8 | -28% | +4% |
| app CPU, M cycles/frame | 1.58 | 1.14 | 1.65 | -28% | +4% |
| app GPU, ms/s | 2.94 | 3.55 | 3.55 | +21% | +21% |
| DWM CPU, M cycles/s | 99.2 | 105 | 108 | +6% | +8% |
| DWM GPU, ms/s | 2.33 | 2.33 | 2.34 | -0% | +1% |
| private, MiB | 134 | 66.5 | 69.8 | -50% | -48% |
| working set, MiB | 104 | 57.3 | 60.1 | -45% | -42% |
| board, W | 15.4 | 14.7 | 14.9 | -4% | -3% |
| pacing p50, ms | 16.7 | 16.7 | 16.7 | | |
| pacing p99, ms | 17.3 | 17.2 | 17.3 | | |
| long intervals | 0.00213 | 0.00209 | 0.00209 | | |

### startup (lab, medians of 10 launches)

| | zgx | zgx-d3d11 | zgx-d3d11-threaded |
| --- | --- | --- | --- |
| to window, ms | 237 | 193 | 214 |
| to content, ms | 280 | 227 | 245 |
| CPU by then, ms | 234 | 203 | 188 |
| cycles by then, M | 814 | 683 | 703 |
