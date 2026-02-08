// Comprehensive test showing all optimizations working together
// EXPECT: 217
int main() {
    // Test 1: Strength reduction on multiply
    int val1 = 7 * 16;  // Should fold to 112, or become 7 << 4
    
    // Test 2: Strength reduction on divide
    int val2 = 128 / 32;  // Should fold to 4, or become 128 >> 5
    
    // Test 3: Strength reduction on modulo
    int val3 = 25 % 8;  // Should fold to 1, or become 25 & 7
    
    // Test 4: Register allocation with multiple live variables
    int a = 10;
    int b = 20;
    int c = 30;
    int d = 40;
    int sum_ab = a + b;
    int sum_cd = c + d;
    int total = sum_ab + sum_cd;
    
    // Test 5: Algebraic simplifications
    int x = total + 0;  // Should eliminate +0
    int y = x * 1;      // Should eliminate *1
    
    // Combine all results
    int result = val1 + val2 + val3 + y;
    
    return result;  // Expected: 112 + 4 + 1 + 100 = 217
}
