# Lumen Cross-Language Benchmark Report

Source: `bench/results/results.csv`

## Environment

- **date**: 2026-10-04T00:44:25Z
- **repo_commit**: bdfa80779ec7
- **repo_dirty**: no
- **lumen**: lumen 0.6.1
- **cpu**: AMD Ryzen 9 6900HX with Radeon Graphics
- **cores**: 16
- **os**: Linux 7.2.8-200.fc44.x86_64 x86_64
- **runs**: 3
- **gcc**: gcc (GCC) 16.2.1 20260819 (Red Hat 16.2.1-2)
- **rustc**: rustc 1.99.0 (b940084d7 2026-09-28)
- **go**: go version go1.26.8 linux/amd64
- **python**: Python 3.14.7

Lumen samples are `lumen run <file>` wall-clock (process start-up and compilation included). Every sample's output was checked against `bench/cross-language/<bench>/expected.txt`; failed or wrong runs are excluded from the tables and listed below.

## Summary (median time in ms)

| Benchmark | c | go | lumen | lumen-interp | python | rust | typescript | Fastest |
|-----------|------:|------:|------:|------:|------:|------:|------:|---------|
| fannkuch | **146** | 153 | 17083 | 17683 | 4650 | 166 | 882 | c |
| fibonacci | **24** | 216 | 97 | 5842 | 1757 | 115 | 1310 | c |
| json_parse | **1** | 13 | 86 | 88 | 23 | 3 | 573 | c |
| matrix_mult | **4** | 8 | - | - | 791 | 8 | 616 | c |
| nbody | 52 | 66 | - | - | 4979 | **44** | 693 | rust |
| primes_sieve | **2** | 4 | 338 | 341 | 198 | 2 | 568 | c |
| sort | 56 | 63 | - | - | 543 | **23** | 789 | rust |
| string_ops | **1** | 2 | 11 | 12 | 13 | 1 | 574 | c |
| tree | 22 | **21** | 426 | 424 | 652 | 25 | 690 | go |

## Relative Performance (vs C baseline)

Values show how many times slower than C (1.0x = same speed).

| Benchmark | c | go | lumen | lumen-interp | python | rust | typescript |
|-----------|------:|------:|------:|------:|------:|------:|------:|
| fannkuch | 1.0x | 1.0x | 117.0x | 121.1x | 31.8x | 1.1x | 6.0x |
| fibonacci | 1.0x | 9.0x | 4.0x | 243.4x | 73.2x | 4.8x | 54.6x |
| json_parse | 1.0x | 13.0x | 86.0x | 88.0x | 23.0x | 3.0x | 573.0x |
| matrix_mult | 1.0x | 2.0x | - | - | 197.8x | 2.0x | 154.0x |
| nbody | 1.0x | 1.3x | - | - | 95.8x | 0.8x | 13.3x |
| primes_sieve | 1.0x | 2.0x | 169.0x | 170.5x | 99.0x | 1.0x | 284.0x |
| sort | 1.0x | 1.1x | - | - | 9.7x | 0.4x | 14.1x |
| string_ops | 1.0x | 2.0x | 11.0x | 12.0x | 13.0x | 1.0x | 574.0x |
| tree | 1.0x | 1.0x | 19.4x | 19.3x | 29.6x | 1.1x | 31.4x |

## Detailed Results

### fannkuch

| Language | Median (ms) | Mean (ms) | Min (ms) | Max (ms) | Stdev | Runs |
|----------|----------:|--------:|-------:|-------:|------:|-----:|
| c | 146.0 | 147.7 | 146.0 | 151.0 | 2.9 | 3 |
| go | 153.0 | 153.3 | 153.0 | 154.0 | 0.6 | 3 |
| lumen | 17083.0 | 17053.3 | 16938.0 | 17139.0 | 103.7 | 3 |
| lumen-interp | 17683.0 | 17583.0 | 17077.0 | 17989.0 | 464.1 | 3 |
| python | 4650.0 | 4656.7 | 4630.0 | 4690.0 | 30.6 | 3 |
| rust | 166.0 | 166.0 | 165.0 | 167.0 | 1.0 | 3 |
| typescript | 882.0 | 860.3 | 803.0 | 896.0 | 50.1 | 3 |

### fibonacci

| Language | Median (ms) | Mean (ms) | Min (ms) | Max (ms) | Stdev | Runs |
|----------|----------:|--------:|-------:|-------:|------:|-----:|
| c | 24.0 | 22.3 | 17.0 | 26.0 | 4.7 | 3 |
| go | 216.0 | 188.0 | 91.0 | 257.0 | 86.5 | 3 |
| lumen | 97.0 | 94.3 | 78.0 | 108.0 | 15.2 | 3 |
| lumen-interp | 5842.0 | 6321.3 | 5819.0 | 7303.0 | 850.2 | 3 |
| python | 1757.0 | 1717.0 | 1508.0 | 1886.0 | 192.2 | 3 |
| rust | 115.0 | 128.0 | 74.0 | 195.0 | 61.5 | 3 |
| typescript | 1310.0 | 2378.0 | 1166.0 | 4658.0 | 1975.8 | 3 |

### json_parse

| Language | Median (ms) | Mean (ms) | Min (ms) | Max (ms) | Stdev | Runs |
|----------|----------:|--------:|-------:|-------:|------:|-----:|
| c | 1.0 | 1.0 | 1.0 | 1.0 | 0.0 | 3 |
| go | 13.0 | 13.3 | 13.0 | 14.0 | 0.6 | 3 |
| lumen | 86.0 | 88.0 | 85.0 | 93.0 | 4.4 | 3 |
| lumen-interp | 88.0 | 88.7 | 87.0 | 91.0 | 2.1 | 3 |
| python | 23.0 | 23.0 | 23.0 | 23.0 | 0.0 | 3 |
| rust | 3.0 | 3.3 | 3.0 | 4.0 | 0.6 | 3 |
| typescript | 573.0 | 614.0 | 571.0 | 698.0 | 72.8 | 3 |

### matrix_mult

| Language | Median (ms) | Mean (ms) | Min (ms) | Max (ms) | Stdev | Runs |
|----------|----------:|--------:|-------:|-------:|------:|-----:|
| c | 4.0 | 4.0 | 4.0 | 4.0 | 0.0 | 3 |
| go | 8.0 | 8.3 | 8.0 | 9.0 | 0.6 | 3 |
| python | 791.0 | 847.3 | 777.0 | 974.0 | 109.9 | 3 |
| rust | 8.0 | 7.7 | 7.0 | 8.0 | 0.6 | 3 |
| typescript | 616.0 | 637.3 | 595.0 | 701.0 | 56.1 | 3 |

### nbody

| Language | Median (ms) | Mean (ms) | Min (ms) | Max (ms) | Stdev | Runs |
|----------|----------:|--------:|-------:|-------:|------:|-----:|
| c | 52.0 | 52.0 | 51.0 | 53.0 | 1.0 | 3 |
| go | 66.0 | 66.3 | 66.0 | 67.0 | 0.6 | 3 |
| python | 4979.0 | 5022.7 | 4952.0 | 5137.0 | 99.9 | 3 |
| rust | 44.0 | 43.7 | 43.0 | 44.0 | 0.6 | 3 |
| typescript | 693.0 | 686.7 | 645.0 | 722.0 | 38.9 | 3 |

### primes_sieve

| Language | Median (ms) | Mean (ms) | Min (ms) | Max (ms) | Stdev | Runs |
|----------|----------:|--------:|-------:|-------:|------:|-----:|
| c | 2.0 | 2.0 | 2.0 | 2.0 | 0.0 | 3 |
| go | 4.0 | 4.0 | 3.0 | 5.0 | 1.0 | 3 |
| lumen | 338.0 | 339.0 | 335.0 | 344.0 | 4.6 | 3 |
| lumen-interp | 341.0 | 341.0 | 338.0 | 344.0 | 3.0 | 3 |
| python | 198.0 | 195.3 | 179.0 | 209.0 | 15.2 | 3 |
| rust | 2.0 | 2.3 | 2.0 | 3.0 | 0.6 | 3 |
| typescript | 568.0 | 583.7 | 556.0 | 627.0 | 38.0 | 3 |

### sort

| Language | Median (ms) | Mean (ms) | Min (ms) | Max (ms) | Stdev | Runs |
|----------|----------:|--------:|-------:|-------:|------:|-----:|
| c | 56.0 | 55.7 | 55.0 | 56.0 | 0.6 | 3 |
| go | 63.0 | 63.3 | 62.0 | 65.0 | 1.5 | 3 |
| python | 543.0 | 545.3 | 540.0 | 553.0 | 6.8 | 3 |
| rust | 23.0 | 22.7 | 22.0 | 23.0 | 0.6 | 3 |
| typescript | 789.0 | 816.3 | 787.0 | 873.0 | 49.1 | 3 |

### string_ops

| Language | Median (ms) | Mean (ms) | Min (ms) | Max (ms) | Stdev | Runs |
|----------|----------:|--------:|-------:|-------:|------:|-----:|
| c | 1.0 | 1.0 | 1.0 | 1.0 | 0.0 | 3 |
| go | 2.0 | 2.3 | 2.0 | 3.0 | 0.6 | 3 |
| lumen | 11.0 | 11.7 | 11.0 | 13.0 | 1.1 | 3 |
| lumen-interp | 12.0 | 12.0 | 12.0 | 12.0 | 0.0 | 3 |
| python | 13.0 | 12.7 | 12.0 | 13.0 | 0.6 | 3 |
| rust | 1.0 | 1.0 | 1.0 | 1.0 | 0.0 | 3 |
| typescript | 574.0 | 600.3 | 574.0 | 653.0 | 45.6 | 3 |

### tree

| Language | Median (ms) | Mean (ms) | Min (ms) | Max (ms) | Stdev | Runs |
|----------|----------:|--------:|-------:|-------:|------:|-----:|
| c | 22.0 | 22.0 | 21.0 | 23.0 | 1.0 | 3 |
| go | 21.0 | 21.0 | 20.0 | 22.0 | 1.0 | 3 |
| lumen | 426.0 | 433.0 | 422.0 | 451.0 | 15.7 | 3 |
| lumen-interp | 424.0 | 424.3 | 419.0 | 430.0 | 5.5 | 3 |
| python | 652.0 | 642.0 | 613.0 | 661.0 | 25.5 | 3 |
| rust | 25.0 | 26.0 | 25.0 | 28.0 | 1.7 | 3 |
| typescript | 690.0 | 723.7 | 632.0 | 849.0 | 112.3 | 3 |

## Failed or wrong runs (excluded above)

| Benchmark | Language | ERROR | WRONG |
|-----------|----------|------:|------:|
| matrix_mult | lumen | 0 | 3 |
| matrix_mult | lumen-interp | 0 | 3 |
| nbody | lumen | 0 | 3 |
| nbody | lumen-interp | 0 | 3 |
| sort | lumen | 0 | 3 |
| sort | lumen-interp | 0 | 3 |

## Lumen Performance Analysis

| Benchmark | Lumen Rank | vs Fastest | Fastest Language |
|-----------|:----------:|:----------:|:----------------:|
| fannkuch | 6/7 | 117.0x | c |
| fibonacci | 2/7 | 4.0x | c |
| json_parse | 5/7 | 86.0x | c |
| primes_sieve | 5/7 | 169.0x | c |
| string_ops | 4/7 | 11.0x | c |
| tree | 5/7 | 20.3x | go |

Average slowdown vs fastest: **67.9x**

---
*Generated by `bench/generate_report.py`*

## Known gaps

The Lumen `sort` (n=100000), `nbody` (different step count) and `matrix_mult` (float formatting) programs do not
print the reference output, so they are listed as WRONG above and excluded from the tables until the sources are
aligned with the other languages. `zig` is not installed on the measuring machine.
