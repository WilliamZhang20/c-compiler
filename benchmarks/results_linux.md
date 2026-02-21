# Benchmark Results (Linux)

Generated: 2026-02-21 13:57:22

| Benchmark | Our Compiler (ms) | GCC -O0 (ms) | GCC -O2 (ms) | Speedup vs GCC-O0 | Speedup vs GCC-O2 |
|-----------|-------------------|--------------|--------------|-------------------|-------------------|
| fib | 1.32 | 1.38 | 1.27 | 1.05x | 0.96x |
| array_sum | 1.36 | 1.27 | 1.36 | 0.93x | 1.00x |
| matmul | 1.39 | 1.38 | 1.40 | 0.99x | 1.01x |
| bitwise | 1.41 | 1.51 | 1.39 | 1.07x | 0.99x |
| struct_bench | 1.28 | 1.25 | 1.27 | 0.98x | 0.99x |

## Notes
- Measurement methodology: 10 warmup runs + 50 measured runs per benchmark
- Times are trimmed mean (remove top/bottom 20%, average middle 60%) to filter outliers
- Speedup > 1.0 means our compiler is faster
- GCC -O0 is no optimization, GCC -O2 is standard optimizations
