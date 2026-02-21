// EXPECT: 42
// Test: comma operator evaluates left-to-right, returns rightmost value

int main() {
    int a = 0;
    int b = 0;

    // Basic comma operator: evaluates both, returns last
    a = (1, 2, 42);

    // Comma in for-loop post expression
    int x = 0;
    int y = 0;
    for (int i = 0; i < 3; i++, x++) {
        y = y + 1;
    }
    // x should be 3, y should be 3

    // Verify side effects happen left-to-right
    int c = 0;
    int d = (c = 10, c + 5);
    // c should be 10, d should be 15

    if (a != 42) return 1;
    if (x != 3) return 2;
    if (y != 3) return 3;
    if (c != 10) return 4;
    if (d != 15) return 5;

    return a; // 42
}
