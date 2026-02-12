# Benchmark Results

Generated: 2026-02-12 00:08:54

| Benchmark | Our Compiler (ms) | GCC -O0 (ms) | GCC -O2 (ms) | Speedup vs GCC-O0 | Speedup vs GCC-O2 |
|-----------|-------------------|--------------|--------------|-------------------|-------------------|
| fib | 12.7 | 12.51 | 12.39 | 0.98x | 0.98x |
| array_sum | 12.25 | 12.24 | 11.91 | 1x | 0.97x |
| matmul | 12.08 | 12.31 | 11.93 | 1.02x | 0.99x |
| bitwise | 12.81 | 12.96 | 13.41 | 1.01x | 1.05x |
| struct_bench | 12.55 | 12.26 | 11.8 | 0.98x | 0.94x |

## Notes
- Times are averaged over 10 runs
- Speedup > 1.0 means our compiler is faster
- GCC -O0 is no optimization, GCC -O2 is standard optimizations

