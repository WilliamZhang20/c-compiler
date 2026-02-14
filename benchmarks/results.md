# Benchmark Results

Generated: 2026-02-14 12:52:10

| Benchmark | Our Compiler (ms) | GCC -O0 (ms) | GCC -O2 (ms) | Speedup vs GCC-O0 | Speedup vs GCC-O2 |
|-----------|-------------------|--------------|--------------|-------------------|-------------------|
| fib | 5.19 | 5.33 | 5.35 | 1.03x | 1.03x |
| array_sum | 4.84 | 4.99 | 5.33 | 1.03x | 1.1x |
| matmul | 4.62 | 5 | 4.66 | 1.08x | 1.01x |
| bitwise | 4.82 | 4.93 | 5.01 | 1.02x | 1.04x |
| struct_bench | 4.87 | 4.89 | 5.05 | 1x | 1.04x |

## Notes
- Measurement methodology: 10 warmup runs + 50 measured runs per benchmark
- Times are trimmed mean (remove top/bottom 20%, average middle 60%) to filter outliers
- Speedup > 1.0 means our compiler is faster
- GCC -O0 is no optimization, GCC -O2 is standard optimizations

