# Benchmark Results (Linux)

Generated: 2026-02-20 21:12:00

| Benchmark | Our Compiler (ms) | GCC -O0 (ms) | GCC -O2 (ms) | Speedup vs GCC-O0 | Speedup vs GCC-O2 |
|-----------|-------------------|--------------|--------------|-------------------|-------------------|
| fib | 1.64 | 1.66 | 1.60 | 1.01x | 0.98x |
| array_sum | 1.52 | 1.52 | 1.50 | 1.00x | 0.99x |
| matmul | 1.55 | 1.58 | 1.58 | 1.02x | 1.02x |
| bitwise | 1.62 | 1.61 | 1.50 | 0.99x | 0.93x |
| struct_bench | 1.46 | 1.45 | 1.42 | 0.99x | 0.97x |

## Notes
- Measurement methodology: 10 warmup runs + 50 measured runs per benchmark
- Times are trimmed mean (remove top/bottom 20%, average middle 60%) to filter outliers
- Speedup > 1.0 means our compiler is faster
- GCC -O0 is no optimization, GCC -O2 is standard optimizations
