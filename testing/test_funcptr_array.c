// EXPECT: 42
// Test array of function pointers and dispatch tables
typedef int (*binop)(int, int);

int add(int a, int b) { return a + b; }
int sub(int a, int b) { return a - b; }
int mul(int a, int b) { return a * b; }
int divide(int a, int b) { return a / b; }

int main() {
    // Array of function pointers via typedef
    binop ops[4];
    ops[0] = add;
    ops[1] = sub;
    ops[2] = mul;
    ops[3] = divide;

    // Dispatch via index
    int r0 = ops[0](10, 5);  // 15
    int r1 = ops[1](10, 5);  // 5
    int r2 = ops[2](10, 5);  // 50
    int r3 = ops[3](10, 5);  // 2

    if (r0 != 15) return 1;
    if (r1 != 5) return 2;
    if (r2 != 50) return 3;
    if (r3 != 2) return 4;

    // Loop over function pointer array
    int results[4];
    for (int i = 0; i < 4; i++) {
        results[i] = ops[i](20, 4);
    }
    // 24, 16, 80, 5
    if (results[0] != 24) return 5;
    if (results[1] != 16) return 6;
    if (results[2] != 80) return 7;
    if (results[3] != 5) return 8;

    // Conditional function pointer selection
    binop chosen;
    if (r0 > r1) {
        chosen = add;
    } else {
        chosen = sub;
    }
    int result = chosen(20, 22); // 42
    if (result != 42) return 9;

    return result; // 42
}
