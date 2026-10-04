# Lumen Cross-Language Benchmark Report

Source: `bench/results/results.csv`

## Environment

- **date**: 2026-10-04T03:02:53Z
- **repo_commit**: 26ddb01894cc
- **repo_dirty**: yes
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
| fannkuch | **149** | 155 | 17118 | 16991 | 4589 | 166 | 885 | c |
| fibonacci | **11** | 39 | 59 | 5541 | 677 | 24 | 654 | c |
| json_parse | **1** | 15 | 82 | 83 | 22 | 3 | 610 | c |
| matrix_mult | **4** | 8 | 870 | 893 | 761 | 8 | 679 | c |
| nbody | 50 | 64 | 8546 | 8646 | 5253 | **42** | 626 | rust |
| primes_sieve | **2** | 3 | 343 | 353 | 183 | 3 | 648 | c |
| sort | 54 | 61 | 381 | 381 | 496 | **22** | 776 | rust |
| string_ops | **1** | 2 | 12 | 12 | 12 | 1 | 554 | c |
| tree | **20** | 20 | 414 | 407 | 608 | 26 | 589 | c |

## Relative Performance (vs C baseline)

Values show how many times slower than C (1.0x = same speed).

| Benchmark | c | go | lumen | lumen-interp | python | rust | typescript |
|-----------|------:|------:|------:|------:|------:|------:|------:|
| fannkuch | 1.0x | 1.0x | 114.9x | 114.0x | 30.8x | 1.1x | 5.9x |
| fibonacci | 1.0x | 3.5x | 5.4x | 503.7x | 61.5x | 2.2x | 59.5x |
| json_parse | 1.0x | 15.0x | 82.0x | 83.0x | 22.0x | 3.0x | 610.0x |
| matrix_mult | 1.0x | 2.0x | 217.5x | 223.2x | 190.2x | 2.0x | 169.8x |
| nbody | 1.0x | 1.3x | 170.9x | 172.9x | 105.1x | 0.8x | 12.5x |
| primes_sieve | 1.0x | 1.5x | 171.5x | 176.5x | 91.5x | 1.5x | 324.0x |
| sort | 1.0x | 1.1x | 7.1x | 7.1x | 9.2x | 0.4x | 14.4x |
| string_ops | 1.0x | 2.0x | 12.0x | 12.0x | 12.0x | 1.0x | 554.0x |
| tree | 1.0x | 1.0x | 20.7x | 20.4x | 30.4x | 1.3x | 29.4x |

## Detailed Results

### fannkuch

| Language | Median (ms) | Mean (ms) | Min (ms) | Max (ms) | Stdev | Runs |
|----------|----------:|--------:|-------:|-------:|------:|-----:|
| c | 149.0 | 148.7 | 147.0 | 150.0 | 1.5 | 3 |
| go | 155.0 | 155.3 | 153.0 | 158.0 | 2.5 | 3 |
| lumen | 17118.0 | 17095.3 | 17039.0 | 17129.0 | 49.1 | 3 |
| lumen-interp | 16991.0 | 16942.3 | 16826.0 | 17010.0 | 101.2 | 3 |
| python | 4589.0 | 4652.3 | 4574.0 | 4794.0 | 122.9 | 3 |
| rust | 166.0 | 165.3 | 164.0 | 166.0 | 1.1 | 3 |
| typescript | 885.0 | 872.3 | 842.0 | 890.0 | 26.4 | 3 |

### fibonacci

| Language | Median (ms) | Mean (ms) | Min (ms) | Max (ms) | Stdev | Runs |
|----------|----------:|--------:|-------:|-------:|------:|-----:|
| c | 11.0 | 10.7 | 10.0 | 11.0 | 0.6 | 3 |
| go | 39.0 | 38.7 | 38.0 | 39.0 | 0.6 | 3 |
| lumen | 59.0 | 58.7 | 57.0 | 60.0 | 1.5 | 3 |
| lumen-interp | 5541.0 | 5566.7 | 5536.0 | 5623.0 | 48.9 | 3 |
| python | 677.0 | 680.7 | 671.0 | 694.0 | 11.9 | 3 |
| rust | 24.0 | 23.0 | 21.0 | 24.0 | 1.7 | 3 |
| typescript | 654.0 | 667.3 | 616.0 | 732.0 | 59.1 | 3 |

### json_parse

| Language | Median (ms) | Mean (ms) | Min (ms) | Max (ms) | Stdev | Runs |
|----------|----------:|--------:|-------:|-------:|------:|-----:|
| c | 1.0 | 1.3 | 1.0 | 2.0 | 0.6 | 3 |
| go | 15.0 | 14.3 | 13.0 | 15.0 | 1.1 | 3 |
| lumen | 82.0 | 84.0 | 82.0 | 88.0 | 3.5 | 3 |
| lumen-interp | 83.0 | 83.3 | 83.0 | 84.0 | 0.6 | 3 |
| python | 22.0 | 22.3 | 22.0 | 23.0 | 0.6 | 3 |
| rust | 3.0 | 3.0 | 3.0 | 3.0 | 0.0 | 3 |
| typescript | 610.0 | 607.7 | 566.0 | 647.0 | 40.5 | 3 |

### matrix_mult

| Language | Median (ms) | Mean (ms) | Min (ms) | Max (ms) | Stdev | Runs |
|----------|----------:|--------:|-------:|-------:|------:|-----:|
| c | 4.0 | 3.7 | 3.0 | 4.0 | 0.6 | 3 |
| go | 8.0 | 8.3 | 8.0 | 9.0 | 0.6 | 3 |
| lumen | 870.0 | 871.3 | 868.0 | 876.0 | 4.2 | 3 |
| lumen-interp | 893.0 | 906.7 | 892.0 | 935.0 | 24.5 | 3 |
| python | 761.0 | 771.7 | 753.0 | 801.0 | 25.7 | 3 |
| rust | 8.0 | 7.7 | 7.0 | 8.0 | 0.6 | 3 |
| typescript | 679.0 | 687.3 | 586.0 | 797.0 | 105.8 | 3 |

### nbody

| Language | Median (ms) | Mean (ms) | Min (ms) | Max (ms) | Stdev | Runs |
|----------|----------:|--------:|-------:|-------:|------:|-----:|
| c | 50.0 | 50.3 | 50.0 | 51.0 | 0.6 | 3 |
| go | 64.0 | 65.0 | 64.0 | 67.0 | 1.7 | 3 |
| lumen | 8546.0 | 8566.0 | 8542.0 | 8610.0 | 38.2 | 3 |
| lumen-interp | 8646.0 | 8799.0 | 8578.0 | 9173.0 | 325.7 | 3 |
| python | 5253.0 | 5185.0 | 4828.0 | 5474.0 | 328.3 | 3 |
| rust | 42.0 | 42.3 | 42.0 | 43.0 | 0.6 | 3 |
| typescript | 626.0 | 666.7 | 617.0 | 757.0 | 78.4 | 3 |

### primes_sieve

| Language | Median (ms) | Mean (ms) | Min (ms) | Max (ms) | Stdev | Runs |
|----------|----------:|--------:|-------:|-------:|------:|-----:|
| c | 2.0 | 2.0 | 2.0 | 2.0 | 0.0 | 3 |
| go | 3.0 | 3.7 | 3.0 | 5.0 | 1.1 | 3 |
| lumen | 343.0 | 344.3 | 335.0 | 355.0 | 10.1 | 3 |
| lumen-interp | 353.0 | 352.0 | 344.0 | 359.0 | 7.5 | 3 |
| python | 183.0 | 186.3 | 179.0 | 197.0 | 9.4 | 3 |
| rust | 3.0 | 2.7 | 2.0 | 3.0 | 0.6 | 3 |
| typescript | 648.0 | 658.0 | 588.0 | 738.0 | 75.5 | 3 |

### sort

| Language | Median (ms) | Mean (ms) | Min (ms) | Max (ms) | Stdev | Runs |
|----------|----------:|--------:|-------:|-------:|------:|-----:|
| c | 54.0 | 54.0 | 54.0 | 54.0 | 0.0 | 3 |
| go | 61.0 | 61.0 | 60.0 | 62.0 | 1.0 | 3 |
| lumen | 381.0 | 380.3 | 378.0 | 382.0 | 2.1 | 3 |
| lumen-interp | 381.0 | 383.3 | 377.0 | 392.0 | 7.8 | 3 |
| python | 496.0 | 498.0 | 496.0 | 502.0 | 3.5 | 3 |
| rust | 22.0 | 22.0 | 22.0 | 22.0 | 0.0 | 3 |
| typescript | 776.0 | 773.3 | 730.0 | 814.0 | 42.1 | 3 |

### string_ops

| Language | Median (ms) | Mean (ms) | Min (ms) | Max (ms) | Stdev | Runs |
|----------|----------:|--------:|-------:|-------:|------:|-----:|
| c | 1.0 | 1.0 | 1.0 | 1.0 | 0.0 | 3 |
| go | 2.0 | 2.3 | 2.0 | 3.0 | 0.6 | 3 |
| lumen | 12.0 | 11.7 | 11.0 | 12.0 | 0.6 | 3 |
| lumen-interp | 12.0 | 11.7 | 11.0 | 12.0 | 0.6 | 3 |
| python | 12.0 | 12.0 | 12.0 | 12.0 | 0.0 | 3 |
| rust | 1.0 | 1.0 | 1.0 | 1.0 | 0.0 | 3 |
| typescript | 554.0 | 587.7 | 545.0 | 664.0 | 66.3 | 3 |

### tree

| Language | Median (ms) | Mean (ms) | Min (ms) | Max (ms) | Stdev | Runs |
|----------|----------:|--------:|-------:|-------:|------:|-----:|
| c | 20.0 | 20.0 | 20.0 | 20.0 | 0.0 | 3 |
| go | 20.0 | 20.3 | 20.0 | 21.0 | 0.6 | 3 |
| lumen | 414.0 | 412.7 | 409.0 | 415.0 | 3.2 | 3 |
| lumen-interp | 407.0 | 408.0 | 406.0 | 411.0 | 2.6 | 3 |
| python | 608.0 | 615.0 | 596.0 | 641.0 | 23.3 | 3 |
| rust | 26.0 | 27.3 | 25.0 | 31.0 | 3.2 | 3 |
| typescript | 589.0 | 615.3 | 589.0 | 668.0 | 45.6 | 3 |

## Lumen Performance Analysis

| Benchmark | Lumen Rank | vs Fastest | Fastest Language |
|-----------|:----------:|:----------:|:----------------:|
| fannkuch | 7/7 | 114.9x | c |
| fibonacci | 4/7 | 5.4x | c |
| json_parse | 5/7 | 82.0x | c |
| matrix_mult | 6/7 | 217.5x | c |
| nbody | 6/7 | 203.5x | rust |
| primes_sieve | 5/7 | 171.5x | c |
| sort | 4/7 | 17.3x | rust |
| string_ops | 4/7 | 12.0x | c |
| tree | 5/7 | 20.7x | c |

Average slowdown vs fastest: **93.9x**

---
*Generated by `bench/generate_report.py`*

## Notes

`zig` is not installed on the measuring machine. Lumen's `nbody` (1M steps) and `fannkuch` are interpreter-bound:
Float cells and list-heavy loops are not JIT-compiled by the strict tier, so `lumen` and `lumen-interp` match there.
