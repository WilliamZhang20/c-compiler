# Benchmark Results

Generated: 2026-02-13 23:23:45

| Benchmark | Our Compiler (ms) | GCC -O0 (ms) | GCC -O2 (ms) | Speedup vs GCC-O0 | Speedup vs GCC-O2 |
|-----------|-------------------|--------------|--------------|-------------------|-------------------|
| fib | 10.79 | 11.08 | 12.19 | 1.03x | 1.13x |
| array_sum | 8.41 | 12.71 | 13.19 | 1.51x | 1.57x |
| matmul | 12.02 | 12.64 | 11.98 | 1.05x | 1x |
| bitwise | 12.95 | 12.85 | 11.8 | 0.99x | 0.91x |
| struct_bench | 11.2 | 11.92 | 12.03 | 1.06x | 1.07x |

## Notes
- Times are averaged over 10 runs
- Speedup > 1.0 means our compiler is faster
- GCC -O0 is no optimization, GCC -O2 is standard optimizations

