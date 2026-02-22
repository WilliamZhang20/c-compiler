// EXPECT: 42
// Test multiple variable declarations, scoping, shadowing
int g = 100;

int main() {
    // Multiple declarations in one statement
    int a = 1, b = 2, c = 3;
    if (a + b + c != 6) return 1;

    // Block scoping
    {
        int a = 10; // shadows outer a
        if (a != 10) return 2;
        {
            int a = 20; // shadows again
            if (a != 20) return 3;
        }
        if (a != 10) return 4; // back to block-scoped a
    }
    if (a != 1) return 5; // back to original a

    // For loop scoping
    int sum = 0;
    for (int i = 0; i < 5; i++) {
        int x = i * 2;
        sum = sum + x;
    }
    // 0+2+4+6+8 = 20
    if (sum != 20) return 6;

    // Variable reuse after scope ends
    {
        int temp = 42;
        sum = temp;
    }
    // temp is out of scope, but sum captured its value
    if (sum != 42) return 7;

    // Global vs local
    int g = 50; // shadows global
    if (g != 50) return 8;

    // Uninitialized then assigned
    int d;
    d = 42;
    if (d != 42) return 9;

    return d; // 42
}
