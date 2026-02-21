// Test __builtin_offsetof
// EXPECT: 42

struct Point {
    int x;
    int y;
    int z;
};

struct Nested {
    char a;
    int b;
    char c;
    int d;
};

int main(void) {
    // offsetof(struct Point, x) should be 0
    int off_x = __builtin_offsetof(struct Point, x);
    
    // offsetof(struct Point, y) should be 4
    int off_y = __builtin_offsetof(struct Point, y);
    
    // offsetof(struct Point, z) should be 8
    int off_z = __builtin_offsetof(struct Point, z);
    
    // Nested struct with alignment padding
    // a: offset 0 (char, 1 byte)
    // padding: 3 bytes for int alignment
    // b: offset 4 (int, 4 bytes)
    // c: offset 8 (char, 1 byte)
    // padding: 3 bytes for int alignment
    // d: offset 12 (int, 4 bytes)
    int off_d = __builtin_offsetof(struct Nested, d);
    
    // 0 + 4 + 8 + 12 = 24
    // But we want to return 42, so: 42 - 24 = 18
    int result = off_x + off_y + off_z + off_d + 18;
    
    return result;
}
