# Benchmark Results (Linux)

Generated: 2026-02-19 23:13:56

| Benchmark | Our Compiler (ms) | GCC -O0 (ms) | GCC -O2 (ms) | Speedup vs GCC-O0 | Speedup vs GCC-O2 |
|-----------|-------------------|--------------|--------------|-------------------|-------------------|
| fib | 1.63 | 1.72 | 1.56 | 1.06x | 0.96x |
| array_sum | 1.52 | 1.52 | 1.56 | 1.00x | 1.03x |
| matmul | 1.53 | 1.53 | 1.55 | 1.00x | 1.01x |
| bitwise | 1.62 | 1.71 | 1.52 | 1.06x | 0.94x |
| struct_bench | 1.53 | 1.55 | 1.49 | 1.01x | 0.97x |

## Notes
- Measurement methodology: 10 warmup runs + 50 measured runs per benchmark
- Times are trimmed mean (remove top/bottom 20%, average middle 60%) to filter outliers
- Speedup > 1.0 means our compiler is faster
- GCC -O0 is no optimization, GCC -O2 is standard optimizations
