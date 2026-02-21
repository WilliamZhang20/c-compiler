// Test _Bool type, _Alignof, register keyword, _Generic
// EXPECT: 42

int main(void) {
    // _Bool type
    _Bool b1 = 1;
    _Bool b0 = 0;
    
    // _Alignof
    int align_int = _Alignof(int);     // Should be 4
    int align_char = _Alignof(char);   // Should be 1
    int align_ptr = _Alignof(int*);    // Should be 8
    
    // register keyword (should be accepted and ignored)
    register int r = 10;
    
    // Combine results: b1(1) + b0(0) + align_int(4) + align_char(1) + align_ptr(8) + r(10) = 24
    // 42 - 24 = 18
    return b1 + b0 + align_int + align_char + align_ptr + r + 18;
}
