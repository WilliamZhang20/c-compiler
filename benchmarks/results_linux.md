# Benchmark Results (Linux)

Generated: 2026-05-26 23:25:13

| Benchmark | Our Compiler (ms) | GCC -O0 (ms) | GCC -O2 (ms) | Speedup vs GCC-O0 | Speedup vs GCC-O2 |
|-----------|-------------------|--------------|--------------|-------------------|-------------------|
| fib | 38.21 | 60.40 | 15.01 | 1.58x | 0.39x |
| array_sum | 4.91 | 93.52 | 4.80 | 19.05x | 0.98x |
| matmul | 6.23 | 62.15 | 6.11 | 9.98x | 0.98x |
| bitwise | 1287.74 | 2164.81 | 323.24 | 1.68x | 0.25x |
| struct_bench | 157.19 | 177.85 | 121.50 | 1.13x | 0.77x |

## Notes
- Measurement methodology: 10 warmup runs + 50 measured runs per benchmark
- Times are trimmed mean (remove top/bottom 20%, average middle 60%) to filter outliers
- Speedup > 1.0 means our compiler is faster
- GCC -O0 is no optimization, GCC -O2 is standard optimizations
