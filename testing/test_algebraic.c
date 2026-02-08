// EXPECT: 42
// Test algebraic simplifications
int main() {
    int x = 42;
    int a = x * 1;    // Should simplify to x
    int b = a + 0;    // Should simplify to a
    int c = b - 0;    // Should simplify to b
    int d = c | 0;    // Should simplify to c
    int e = d & -1;   // Should simplify to d (all bits set)
    int f = e ^ 0;    // Should simplify to e
    int g = f << 0;   // Should simplify to f
    int h = g >> 0;   // Should simplify to g
    int i = h / 1;    // Should simplify to h
    int j = i % 1;    // Should simplify to 0
    int k = j + 42;   // Should be 0 + 42 = 42
    return k;
}
