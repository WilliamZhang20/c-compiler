// EXPECT: 20
// Comprehensive test of all qualifiers
const int CONST_GLOBAL = 10;
volatile int volatile_global = 5;

int multiply(int a, int b) {
    return a * b;
}

int main() {
    const int local_const = 3;
    volatile int local_volatile = 7;
    
    int x = 20;
    int * restrict ptr = &x;
    
    // Use const and multiply
    return multiply(CONST_GLOBAL, 2);  // 10 * 2 = 20
}
