// EXPECT: 42
// Test: typeof / __typeof__ type specifier

int main() {
    int x = 42;

    // typeof(type) — declares y with the same type as int
    typeof(int) y = 10;
    if (y != 10) return 1;

    // typeof(expr) — declares z with the same type as x (int)
    __typeof__(x) z = 32;
    if (z != 32) return 2;

    // typeof in arithmetic
    typeof(x) sum = y + z;
    if (sum != 42) return 3;

    return sum; // 42
}
