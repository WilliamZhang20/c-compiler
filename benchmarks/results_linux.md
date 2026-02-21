# Benchmark Results (Linux)

Generated: 2026-02-21 15:30:11

| Benchmark | Our Compiler (ms) | GCC -O0 (ms) | GCC -O2 (ms) | Speedup vs GCC-O0 | Speedup vs GCC-O2 |
|-----------|-------------------|--------------|--------------|-------------------|-------------------|
| fib | 1.37 | 1.37 | 1.34 | 1.00x | 0.98x |
| array_sum | 1.36 | 1.39 | 1.42 | 1.02x | 1.04x |
| matmul | 1.36 | 1.41 | 1.35 | 1.04x | 0.99x |
| bitwise | 1.51 | 1.56 | 1.43 | 1.03x | 0.95x |
| struct_bench | 1.37 | 1.39 | 1.40 | 1.01x | 1.02x |

## Notes
- Measurement methodology: 10 warmup runs + 50 measured runs per benchmark
- Times are trimmed mean (remove top/bottom 20%, average middle 60%) to filter outliers
- Speedup > 1.0 means our compiler is faster
- GCC -O0 is no optimization, GCC -O2 is standard optimizations
