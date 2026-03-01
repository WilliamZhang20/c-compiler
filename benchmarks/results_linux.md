# Benchmark Results (Linux)

Generated: 2026-03-01 18:16:10

| Benchmark | Our Compiler (ms) | GCC -O0 (ms) | GCC -O2 (ms) | Speedup vs GCC-O0 | Speedup vs GCC-O2 |
|-----------|-------------------|--------------|--------------|-------------------|-------------------|
| fib | 35.21 | 54.91 | 13.62 | 1.56x | 0.39x |
| array_sum | 4.46 | 85.91 | 4.46 | 19.26x | 1.00x |
| matmul | 7.73 | 51.41 | 5.46 | 6.65x | 0.71x |
| bitwise | 1255.42 | 2066.41 | 312.14 | 1.65x | 0.25x |
| struct_bench | 144.73 | 162.49 | 112.63 | 1.12x | 0.78x |

## Notes
- Measurement methodology: 10 warmup runs + 50 measured runs per benchmark
- Times are trimmed mean (remove top/bottom 20%, average middle 60%) to filter outliers
- Speedup > 1.0 means our compiler is faster
- GCC -O0 is no optimization, GCC -O2 is standard optimizations
