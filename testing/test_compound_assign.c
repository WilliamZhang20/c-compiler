// EXPECT: 0
int main() {
    int a = 10;
    a += 5;
    if (a != 15) return 1;
    
    a -= 5;
    if (a != 10) return 2;
    
    a *= 2;
    if (a != 20) return 3;
    
    a /= 2;
    if (a != 10) return 4;
    
    a %= 3;
    if (a != 1) return 5;
    
    int b = 3; // binary 011
    b <<= 2;   // binary 1100 = 12
    if (b != 12) return 6;
    
    b >>= 1;   // binary 0110 = 6
    if (b != 6) return 7;
    
    b &= 3;    // 6 & 3 = 110 & 011 = 010 = 2
    if (b != 2) return 8;
    
    b |= 4;    // 2 | 4 = 010 | 100 = 110 = 6
    if (b != 6) return 9;
    
    b ^= 6;    // 6 ^ 6 = 0
    if (b != 0) return 10;
    
    // Pointer arithmetic
    int arr[5];
    arr[0] = 100;
    arr[1] = 200;
    
    int* p = arr;
    if (*p != 100) return 11;
    
    p += 1;
    if (*p != 200) return 12;
    
    p -= 1;
    if (*p != 100) return 13;
    
    return 0;
}
