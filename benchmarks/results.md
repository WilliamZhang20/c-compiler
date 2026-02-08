# Benchmark Results

Generated: 2026-02-07 21:04:12

| Benchmark | Our Compiler (ms) | GCC -O0 (ms) | GCC -O2 (ms) | Speedup vs GCC-O0 | Speedup vs GCC-O2 |
|-----------|-------------------|--------------|--------------|-------------------|-------------------|
| fib | 24 | 19.93 | 20.2 | 0.83x | 0.84x |
| array_sum | 21.02 | 18.66 | 17.7 | 0.89x | 0.84x |
| matmul | 21.59 | 17.81 | 16.33 | 0.82x | 0.76x |
| bitwise | 23.17 | 19.45 | 15.05 | 0.84x | 0.65x |
| struct_bench | 18.81 | 17.69 | 18.41 | 0.94x | 0.98x |

## Notes
- Times are averaged over 10 runs
- Speedup > 1.0 means our compiler is faster
- GCC -O0 is no optimization, GCC -O2 is standard optimizations

