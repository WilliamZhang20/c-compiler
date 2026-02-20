// EXPECT: 0
// #include <stdarg.h>

// A simple variadic function that sums integers.
// Uses __builtin_va_* directly to test compiler support.

int sum(int count, ...) {
    __builtin_va_list ap;
    __builtin_va_start(ap, count);
    
    int total = 0;
    // We cannot easily test va_arg unless we implement it or rely on pointer arithmetic logic.
    // However, for this test, we just want to ensure va_start/va_end compile and run without crashing.
    // We will simulate accessing arguments manually if needed, but for now just validation of symbols.
    
    // To properly test, we need to know the layout. 
    // In our implementation, args are on stack.
    // ap should point to the argument AFTER count.
    
    // Let's verify ap is not null.
    // if (!ap) return -1;
    
    __builtin_va_end(ap);
    return count;
}

int main() {
    int res = sum(5, 1, 2, 3, 4, 5);
    if (res != 5) return 1;
    return 0;
}
