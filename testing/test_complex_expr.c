// EXPECT: 42
// Test complex integer expressions: precedence, associativity, mixed operations
int main() {
    // Operator precedence: * before +
    int a = 2 + 3 * 4; // 14, not 20
    if (a != 14) return 1;

    // Parentheses override precedence
    int b = (2 + 3) * 4; // 20
    if (b != 20) return 2;

    // Left-to-right associativity
    int c = 100 - 50 - 25; // 25, not 75
    if (c != 25) return 3;

    // Mixed arithmetic and bitwise
    int d = (0xFF & 0x0F) | (0xF0 & 0xFF); // 0x0F | 0xF0 = 0xFF = 255
    if (d != 255) return 4;

    // Shift precedence
    int e = 1 << 3 + 1; // 1 << 4 = 16 (+ binds tighter than <<)
    if (e != 16) return 5;

    // Comparison produces 0 or 1
    int f = (10 > 5) + (3 < 7) + (1 == 1); // 1 + 1 + 1 = 3
    if (f != 3) return 6;

    // Logical operators
    int g = (1 && 1) + (0 || 1) + (1 && 0); // 1 + 1 + 0 = 2
    if (g != 2) return 7;

    // Complex nested expression
    int x = 5, y = 10, z = 15;
    int h = (x + y) * (z - x) / (y - x); // 15 * 10 / 5 = 30
    if (h != 30) return 8;

    // Modulo and division
    int i = 100 % 7; // 2
    if (i != 2) return 9;

    // Negative numbers
    int j = -10 + 15; // 5
    if (j != 5) return 10;

    int k = -(-42); // 42
    if (k != 42) return 11;

    // Chained assignment
    int p, q, r;
    p = q = r = 42;
    if (p != 42 || q != 42 || r != 42) return 12;

    return p; // 42
}
