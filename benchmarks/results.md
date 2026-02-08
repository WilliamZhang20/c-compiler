# Benchmark Results

Generated: 2026-02-08 09:08:34

| Benchmark | Our Compiler (ms) | GCC -O0 (ms) | GCC -O2 (ms) | Speedup vs GCC-O0 | Speedup vs GCC-O2 |
|-----------|-------------------|--------------|--------------|-------------------|-------------------|
| fib | 15.23 | 14.32 | 14.61 | 0.94x | 0.96x |
| array_sum | 12.34 | 14.18 | 12.13 | 1.15x | 0.98x |
| matmul | 13.27 | 12.52 | 14.33 | 0.94x | 1.08x |
| bitwise | 14.64 | 12.69 | 13.11 | 0.87x | 0.9x |
| struct_bench | 14.24 | 15.02 | 13.43 | 1.05x | 0.94x |

## Notes
- Times are averaged over 10 runs
- Speedup > 1.0 means our compiler is faster
- GCC -O0 is no optimization, GCC -O2 is standard optimizations

