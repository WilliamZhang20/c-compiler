// Test file to verify all optimizations work correctly
// EXPECT: 155
int main() {
    int result = 0;
    
    // Test strength reduction: multiply by power of 2
    int a = 5;
    int b = a * 8;  // Should become: a << 3
    result = result + b;
    
    // Test strength reduction: divide by power of 2
    int c = 64;
    int d = c / 16;  // Should become: c >> 4
    result = result + d;
    
    // Test strength reduction: mod by power of 2
    int e = 17;
    int f = e % 4;  // Should become: e & 3
    result = result + f;
    
    // Test register allocation: multiple local variables
    int x1 = 10;
    int x2 = 20;
    int x3 = 30;
    int x4 = 40;
    int x5 = x1 + x2;
    int x6 = x3 + x4;
    int x7 = x5 + x6;
    result = result + x7;
    
    // Test constant folding
    int const1 = 2 + 3;  // Should fold to 5
    int const2 = const1 * 2;  // Should fold to 10
    result = result + const2;
    
    // Test algebraic simplifications
    int zero_add = result + 0;  // Should eliminate
    int mul_one = zero_add * 1;  // Should eliminate
    
    return mul_one;  // Expected: 40 + 4 + 1 + 100 + 10 = 155
}
