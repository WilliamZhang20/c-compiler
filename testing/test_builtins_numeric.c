// Test __builtin_clz, __builtin_ctz, __builtin_popcount
// EXPECT: 42

int main(void) {
    // __builtin_clz: count leading zeros (32-bit)
    int clz = __builtin_clz(1);       // 31 leading zeros
    int clz2 = __builtin_clz(256);    // 23 leading zeros (bit 8 set)
    
    // __builtin_ctz: count trailing zeros
    int ctz = __builtin_ctz(8);       // 3 trailing zeros
    
    // __builtin_popcount: count set bits
    int pop = __builtin_popcount(255); // 8 bits set
    
    // __builtin_abs
    int abs_val = __builtin_abs(-5);   // 5
    
    // clz(31) + clz2(23) + ctz(3) + pop(8) + abs(5) = 70
    // 42 = 70 - 28
    return clz + clz2 + ctz + pop + abs_val - 28;
}
