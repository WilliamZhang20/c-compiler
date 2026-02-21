// EXPECT: 42
// Test: ternary with omitted middle operand (GNU extension a ?: b)

int main() {
    // Non-zero condition: result is the condition value itself
    int a = 42 ?: 99;
    if (a != 42) return 1;

    // Zero condition: result is the else value
    int b = 0 ?: 10;
    if (b != 10) return 2;

    // Variable form
    int x = 42;
    int c = x ?: 0;
    if (c != 42) return 3;

    int y = 0;
    int d = y ?: 42;
    if (d != 42) return 4;

    return a; // 42
}
