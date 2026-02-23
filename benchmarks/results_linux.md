# Benchmark Results (Linux)

Generated: 2026-02-23 17:42:07

| Benchmark | Our Compiler (ms) | GCC -O0 (ms) | GCC -O2 (ms) | Speedup vs GCC-O0 | Speedup vs GCC-O2 |
|-----------|-------------------|--------------|--------------|-------------------|-------------------|
| fib | 1.51 | 1.60 | 1.49 | 1.06x | 0.99x |
| array_sum | 1.57 | 1.57 | 1.62 | 1.00x | 1.03x |
| matmul | 1.38 | 1.41 | 1.34 | 1.02x | 0.97x |
| bitwise | 1.44 | 1.48 | 1.39 | 1.03x | 0.97x |
| struct_bench | 1.37 | 1.43 | 1.36 | 1.04x | 0.99x |

## Notes
- Measurement methodology: 10 warmup runs + 50 measured runs per benchmark
- Times are trimmed mean (remove top/bottom 20%, average middle 60%) to filter outliers
- Speedup > 1.0 means our compiler is faster
- GCC -O0 is no optimization, GCC -O2 is standard optimizations
