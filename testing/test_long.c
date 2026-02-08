// EXPECT: 15
// Test long and long long integer types
int main() {
    long a = 1000000000;           // 1 billion
    long b = 2000000000;           // 2 billion
    
    long long big = 9000000000;    // 9 billion (fits in long long)
    long long small = 1;
    
    short s = 100;
    short t = -50;
    short sum = s + t;             // Should be 50
    
    unsigned long ul = 12345678;
    unsigned long long ull = 999999999;
    
    // Return a simple value
    short x = 10;
    short y = 5;
    return x + y;  // 15
}
