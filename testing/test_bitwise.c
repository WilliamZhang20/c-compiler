// EXPECT: 7
int main() {
    int a = 5;      // 0101
    int b = 3;      // 0011
    
    int bit_and = a & b;     // 0001 (1)
    int bit_or  = a | b;     // 0111 (7)
    int bit_xor = a ^ b;     // 0110 (6)
    int shift_l = 1 << 2;    // 0100 (4)
    int shift_r = 8 >> 1;    // 0100 (4)
    int mod     = 10 % 3;    // 1
    
    // 7 & 6 = 6
    // 6 ^ 4 = 2
    // 2 | 1 = 3
    // 3 + 4 = 7
    return (bit_or & bit_xor) ^ shift_l | mod + shift_r;
}
