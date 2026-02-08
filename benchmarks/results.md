# Benchmark Results

Generated: 2026-02-08 16:46:53

| Benchmark | Our Compiler (ms) | GCC -O0 (ms) | GCC -O2 (ms) | Speedup vs GCC-O0 | Speedup vs GCC-O2 |
|-----------|-------------------|--------------|--------------|-------------------|-------------------|
| fib | 20.65 | 17.31 | 19.4 | 0.84x | 0.94x |
| array_sum | 20.54 | 19.49 | 21.51 | 0.95x | 1.05x |
| matmul | 18.25 | 17.98 | 18.44 | 0.99x | 1.01x |
| bitwise | 22.88 | 21.38 | 20.26 | 0.93x | 0.89x |
| struct_bench | 18.39 | 18.36 | 17.18 | 1x | 0.93x |

## Notes
- Times are averaged over 10 runs
- Speedup > 1.0 means our compiler is faster
- GCC -O0 is no optimization, GCC -O2 is standard optimizations

