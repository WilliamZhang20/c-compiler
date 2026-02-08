// EXPECT: 42
// Test showcasing all optimizations:
// - Strength reduction (multiply/divide by powers of 2)
// - Copy propagation (eliminate redundant copies)
// - Dead store elimination (remove unused variables)
// - Common subexpression elimination (reuse computed values)
// - Constant folding
// - Peephole optimization (remove redundant moves, use LEA, etc.)

int main() {
    // Strength reduction: multiply by 8 -> shift left by 3
    int a = 5 * 8;  // Should become: 5 << 3
    
    // Copy propagation: eliminate intermediate copy
    int b = a;
    int c = b;  // c should directly use a
    
    // Common subexpression elimination: reuse 3 + 4
    int d = 3 + 4;
    int e = 3 + 4;  // Should reuse result of d
    
    // Dead store elimination: f is never used
    int f = 100;
    
    // Constant folding: 2 + 3 -> 5
    int g = 2 + 3;
    
    // Division by power of 2: divide by 4 -> shift right by 2
    int h = a / 4;  // Should become: a >> 2
    
    // Result: 40 + 7 - 5 = 42
    return c + d - g;
}
