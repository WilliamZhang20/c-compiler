# Benchmark Results (Linux)

Generated: 2026-02-20 21:34:26

| Benchmark | Our Compiler (ms) | GCC -O0 (ms) | GCC -O2 (ms) | Speedup vs GCC-O0 | Speedup vs GCC-O2 |
|-----------|-------------------|--------------|--------------|-------------------|-------------------|
| fib | 1.70 | 1.65 | 1.62 | 0.97x | 0.95x |
| array_sum | 1.58 | 1.62 | 1.56 | 1.03x | 0.99x |
| matmul | 1.61 | 1.66 | 1.64 | 1.03x | 1.02x |
| bitwise | 1.64 | 1.68 | 1.53 | 1.02x | 0.93x |
| struct_bench | 1.56 | 1.58 | 1.56 | 1.01x | 1.00x |

## Notes
- Measurement methodology: 10 warmup runs + 50 measured runs per benchmark
- Times are trimmed mean (remove top/bottom 20%, average middle 60%) to filter outliers
- Speedup > 1.0 means our compiler is faster
- GCC -O0 is no optimization, GCC -O2 is standard optimizations
