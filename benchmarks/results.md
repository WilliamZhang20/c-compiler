# Benchmark Results

Generated: 2026-02-14 12:40:14

| Benchmark | Our Compiler (ms) | GCC -O0 (ms) | GCC -O2 (ms) | Speedup vs GCC-O0 | Speedup vs GCC-O2 |
|-----------|-------------------|--------------|--------------|-------------------|-------------------|
| fib | 15.49 | 15.55 | 14.34 | 1x | 0.93x |
| array_sum | 15.2 | 15.02 | 16.39 | 0.99x | 1.08x |
| matmul | 14.11 | 14.16 | 15.21 | 1x | 1.08x |
| bitwise | 13.79 | 14.79 | 14.41 | 1.07x | 1.04x |
| struct_bench | 15.43 | 15.39 | 16.77 | 1x | 1.09x |

## Notes
- Times are averaged over 10 runs
- Speedup > 1.0 means our compiler is faster
- GCC -O0 is no optimization, GCC -O2 is standard optimizations

