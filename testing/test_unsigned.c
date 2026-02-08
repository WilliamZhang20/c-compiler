// EXPECT: 55
// Test unsigned integer types
int main() {
    unsigned int a = 10;
    unsigned int b = 5;
    unsigned int c = a + b;        // 15
    
    unsigned char x = 100;
    unsigned char y = 50;
    unsigned char z = x - y;       // 50
    
    unsigned short s = 30;
    unsigned short t = 20;
    unsigned short u = s - t;      // 10
    
    // Test that unsigned comparison works correctly
    unsigned int d = 5;
    unsigned int e = 10;
    if (d < e) {
        return c + z + u - t;  // 15 + 50 + 10 - 20 = 55
    }
    
    return 0;
}
