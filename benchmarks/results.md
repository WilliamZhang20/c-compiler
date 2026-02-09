# Benchmark Results

Generated: 2026-02-08 20:21:00

| Benchmark | Our Compiler (ms) | GCC -O0 (ms) | GCC -O2 (ms) | Speedup vs GCC-O0 | Speedup vs GCC-O2 |
|-----------|-------------------|--------------|--------------|-------------------|-------------------|
| fib | 20.87 | 20.34 | 24.26 | 0.97x | 1.16x |
| array_sum | 19.48 | 17.02 | 17.18 | 0.87x | 0.88x |
| matmul | 23.93 | 20.63 | 23.86 | 0.86x | 1x |
| bitwise | 19.78 | 19.01 | 22.32 | 0.96x | 1.13x |
| struct_bench | 21.93 | 24.27 | 22.66 | 1.11x | 1.03x |

## Notes
- Times are averaged over 10 runs
- Speedup > 1.0 means our compiler is faster
- GCC -O0 is no optimization, GCC -O2 is standard optimizations

