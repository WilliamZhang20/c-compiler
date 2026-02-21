// Test _Generic selection
// EXPECT: 42

int main(void) {
    int x = 10;
    float f = 3.14;
    
    // _Generic selecting based on type of controlling expression
    int r1 = _Generic(x, int: 20, float: 30, default: 40);   // Should be 20 (int)
    int r2 = _Generic(f, int: 5, float: 12, default: 99);     // Should be 12 (float)
    int r3 = _Generic(x, float: 1, default: 10);              // Should be 10 (default)
    
    return r1 + r2 + r3;  // 20 + 12 + 10 = 42
}
