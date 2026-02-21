// EXPECT: 42
// Test: __builtin_expect (used by likely/unlikely macros)

int main() {
    int x = 42;

    // __builtin_expect should return the first argument unchanged
    int a = __builtin_expect(x, 1);
    if (a != 42) return 1;

    // Common pattern: if (__builtin_expect(cond, 0)) — unlikely branch
    if (__builtin_expect(x == 42, 1)) {
        // likely path
    } else {
        return 2;
    }

    // __builtin_constant_p should return 0 (not a constant in our simple compiler)
    int b = __builtin_constant_p(42);
    if (b != 0) return 3;

    return a; // 42
}
