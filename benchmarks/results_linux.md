# Benchmark Results (Linux)

Generated: 2026-06-01 23:01:16

| Benchmark | Our Compiler (ms) | GCC -O0 (ms) | GCC -O2 (ms) | GCC -O3 (ms) | Speedup vs GCC-O0 | Speedup vs GCC-O2 | Speedup vs GCC-O3 |
|-----------|-------------------|--------------|--------------|--------------|-------------------|-------------------|-------------------|
| fib | 3.08 | 2.89 | 2.97 | 3.00 | 0.94x | 0.96x | 0.97x |
| array_sum | 9.35 | 87.53 | 8.17 | 7.31 | 9.36x | 0.87x | 0.78x |
| matmul | 9.25 | 72.85 | 7.74 | 7.59 | 7.88x | 0.84x | 0.82x |
| bitwise | 14.52 | 33.55 | 11.78 | 11.90 | 2.31x | 0.81x | 0.82x |
| struct_bench | 192.47 | 215.69 | 144.04 | 142.09 | 1.12x | 0.75x | 0.74x |

## Notes
- Measurement methodology: 10 warmup runs + 50 measured runs per benchmark
- Times are trimmed mean (remove top/bottom 20%, average middle 60%) to filter outliers
- Speedup > 1.0 means our compiler is faster
- GCC builds use -mpopcnt so __builtin_popcount lowers to the popcnt instruction (same baseline as this compiler)
- fib is an iterative O(n) loop in source (not recursive); compares loop/codegen fairly, not recurrence elimination
- GCC -O0 is no optimization; -O2 is standard optimizations; -O3 adds aggressive inlining and vectorization
