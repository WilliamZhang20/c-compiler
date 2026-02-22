// EXPECT: 42
// Test float arithmetic, comparisons, casting, and edge cases
int main() {
    // Float addition
    float f1 = 1.5;
    float f2 = 2.5;
    float f3 = f1 + f2; // 4.0
    if ((int)f3 != 4) return 1;

    // Float subtraction
    float f4 = 10.0;
    float f5 = 3.5;
    float f6 = f4 - f5; // 6.5
    if ((int)f6 != 6) return 2;

    // Float multiplication
    float m1 = 6.0;
    float m2 = 7.0;
    float prod = m1 * m2; // 42.0
    if ((int)prod != 42) return 3;

    // Float division
    float a = 10.0;
    float b = 3.0;
    float div = a / b; // ~3.333
    int idiv = (int)div;
    if (idiv != 3) return 4;

    // Negative floats
    float neg = -5.5;
    int ineg = (int)neg; // -5 (truncation toward zero)
    if (ineg != -5) return 5;

    // Float comparison operators
    float x = 1.0;
    float y = 2.0;
    if (!(x < y)) return 6;
    if (!(y > x)) return 7;
    if (x == y) return 8;
    if (!(x != y)) return 9;
    if (x >= y) return 10;
    if (y <= x) return 11;

    // Float equality
    float z1 = 3.0;
    float z2 = 3.0;
    if (z1 != z2) return 12;

    // Int to float cast
    int big = 100;
    float fbig = (float)big;
    int back = (int)fbig;
    if (back != 100) return 13;

    return (int)prod; // 42
}
