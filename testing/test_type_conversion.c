// EXPECT: 42
// Test integer type conversions and promotions
int main() {
    // char to int promotion
    char c = 10;
    int i = c + 32; // char promoted to int for arithmetic
    if (i != 42) return 1;

    // short to int
    short s = 100;
    int si = s * 2;
    if (si != 200) return 2;

    // Unsigned to signed
    unsigned int u = 50;
    int su = (int)u;
    if (su != 50) return 3;

    // Negative cast to unsigned and back
    int neg = -1;
    unsigned int un = (unsigned int)neg; // 0xFFFFFFFF
    if (un != 4294967295U) return 4;

    // Truncation: int to char
    int big = 300;
    char small = (char)big; // 300 % 256 = 44
    if (small != 44) return 5;

    // Widening: char to long
    char ch = 42;
    long l = (long)ch;
    if (l != 42) return 6;

    // Shift with mixed types
    int base = 1;
    char shift = 4;
    int shifted = base << shift; // char operand promoted
    if (shifted != 16) return 7;

    // Comparison between signed and unsigned
    int signed_val = 5;
    unsigned int unsigned_val = 5;
    if (signed_val != (int)unsigned_val) return 8;

    // Unsigned arithmetic
    unsigned int ua = 10;
    unsigned int ub = 3;
    unsigned int uc = ua / ub; // 3
    unsigned int ud = ua % ub; // 1
    if (uc != 3) return 9;
    if (ud != 1) return 10;

    return 42;
}
