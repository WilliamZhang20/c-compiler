# Benchmark Results

Generated: 2026-02-13 22:47:19

| Benchmark | Our Compiler (ms) | GCC -O0 (ms) | GCC -O2 (ms) | Speedup vs GCC-O0 | Speedup vs GCC-O2 |
|-----------|-------------------|--------------|--------------|-------------------|-------------------|
| fib | 12.59 | 12.73 | 11.87 | 1.01x | 0.94x |
| array_sum | 11.54 | 12.27 | 12.22 | 1.06x | 1.06x |
| matmul | 13.22 | 12.64 | 11.98 | 0.96x | 0.91x |
| bitwise | 11.79 | 12.37 | 11.92 | 1.05x | 1.01x |
| struct_bench | 10.88 | 11.83 | 11 | 1.09x | 1.01x |

## Notes
- Times are averaged over 10 runs
- Speedup > 1.0 means our compiler is faster
- GCC -O0 is no optimization, GCC -O2 is standard optimizations

