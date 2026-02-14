# Benchmark Results

Generated: 2026-02-13 23:02:48

| Benchmark | Our Compiler (ms) | GCC -O0 (ms) | GCC -O2 (ms) | Speedup vs GCC-O0 | Speedup vs GCC-O2 |
|-----------|-------------------|--------------|--------------|-------------------|-------------------|
| fib | 11.63 | 12.33 | 11.8 | 1.06x | 1.01x |
| array_sum | 11.29 | 12.19 | 12.44 | 1.08x | 1.1x |
| matmul | 16.61 | 12.91 | 13.5 | 0.78x | 0.81x |
| bitwise | 14.36 | 15.65 | 15.35 | 1.09x | 1.07x |
| struct_bench | 10.64 | 11.42 | 11.83 | 1.07x | 1.11x |

## Notes
- Times are averaged over 10 runs
- Speedup > 1.0 means our compiler is faster
- GCC -O0 is no optimization, GCC -O2 is standard optimizations

