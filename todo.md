
## Future Feature Roadmap

To fully compile big projects like the Linux Kernel, the following features are prioritized:

### Section 1: Advanced Linkage
- `extern "C"` linkage (if C++ interop needed)
- Weak symbols (`__attribute__((weak))`)
- Symbol versioning and aliases

### Section 2: GNU Extensions
- Statement expressions (`({ ... })`)
- `typeof` operator
- Compound literals
- Designated initializers for arrays/structs

### Section 3: Type System Edge Cases
- Type qualifiers on function parameters
- Complex array declarators
- Function pointer syntax edge cases

### Section 4: Floating-Point Robustness
- Proper NaN/Inf handling
- Floating-point precision directives
- SSE/AVX vector operations (for kernel SIMD)