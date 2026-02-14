# Benchmark Results

Generated: 2026-02-13 22:21:26

| Benchmark | Our Compiler (ms) | GCC -O0 (ms) | GCC -O2 (ms) | Speedup vs GCC-O0 | Speedup vs GCC-O2 |
|-----------|-------------------|--------------|--------------|-------------------|-------------------|
| fib | 16.38 | 15.61 | 14.23 | 0.95x | 0.87x |
| array_sum | 17.52 | 13.4 | 14.99 | 0.76x | 0.86x |
| matmul | 15.08 | 15.87 | 15.11 | 1.05x | 1x |
| bitwise | 14.7 | 14.83 | 15 | 1.01x | 1.02x |
| struct_bench | 14.88 | 13.92 | 15.13 | 0.94x | 1.02x |

## Notes
- Times are averaged over 10 runs
- Speedup > 1.0 means our compiler is faster
- GCC -O0 is no optimization, GCC -O2 is standard optimizations

