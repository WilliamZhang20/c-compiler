// EXPECT: 42
// Test float AND double arithmetic, comparisons, casting, and mixed operations
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

    // === DOUBLE tests ===

    // Double addition
    double d1 = 1.5;
    double d2 = 2.5;
    double d3 = d1 + d2; // 4.0
    if ((int)d3 != 4) return 14;

    // Double subtraction
    double d4 = 10.0;
    double d5 = 3.5;
    double d6 = d4 - d5; // 6.5
    if ((int)d6 != 6) return 15;

    // Double multiplication
    double dm1 = 6.0;
    double dm2 = 7.0;
    double dprod = dm1 * dm2; // 42.0
    if ((int)dprod != 42) return 16;

    // Double division
    double da = 10.0;
    double db = 3.0;
    double ddiv = da / db; // ~3.333
    int didiv = (int)ddiv;
    if (didiv != 3) return 17;

    // Negative doubles
    double dneg = -5.5;
    int dineg = (int)dneg;
    if (dineg != -5) return 18;

    // Double comparisons
    double dx = 1.0;
    double dy = 2.0;
    if (!(dx < dy)) return 19;
    if (!(dy > dx)) return 20;
    if (dx == dy) return 21;

    // Int to double cast
    int ibig = 100;
    double dbig = (double)ibig;
    int dback = (int)dbig;
    if (dback != 100) return 22;

    // Float-double mixed: float + double should promote to double
    float fmix = 1.5;
    double dmix = 2.5;
    double mixed = fmix + dmix; // 4.0
    if ((int)mixed != 4) return 23;

    return (int)prod; // 42
}
