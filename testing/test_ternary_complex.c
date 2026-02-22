// EXPECT: 42
// Test deeply nested ternary, complex conditional expressions, and function pointers
int abs_val(int x) {
    return x < 0 ? -x : x;
}

int clamp(int x, int lo, int hi) {
    return x < lo ? lo : (x > hi ? hi : x);
}

int add(int a, int b) { return a + b; }
int sub(int a, int b) { return a - b; }

int main() {
    // Basic ternary
    int a = 1 ? 10 : 20;
    if (a != 10) return 1;
    int b = 0 ? 10 : 20;
    if (b != 20) return 2;

    // Nested ternary (simulate multi-way dispatch)
    int x = 2;
    int r = x == 0 ? 100
          : x == 1 ? 200
          : x == 2 ? 300
          : 400;
    if (r != 300) return 3;

    // Ternary with side effects
    int c = 0, d = 0;
    int sel = 1;
    sel ? (c = 5) : (d = 5);
    if (c != 5) return 4;
    if (d != 0) return 5;

    // Function returning via ternary
    if (abs_val(-7) != 7) return 6;
    if (abs_val(3) != 3) return 7;

    // Clamp function using nested ternary
    if (clamp(5, 0, 10) != 5) return 8;
    if (clamp(-5, 0, 10) != 0) return 9;
    if (clamp(15, 0, 10) != 10) return 10;

    // Ternary in array index
    int arr[3];
    arr[0] = 10; arr[1] = 42; arr[2] = 30;
    int idx = 1;
    int val = arr[idx > 0 ? idx : 0];
    if (val != 42) return 11;

    // Ternary with function pointer operands (then path)
    int (*op)(int, int) = (x > 0) ? add : sub;
    int fptr_res = op(20, 22); // x=2 > 0, so add(20,22) = 42
    if (fptr_res != 42) return 12;

    // Ternary with function pointer operands (else path)
    int (*op2)(int, int) = (x < 0) ? add : sub;
    int fptr_res2 = op2(50, 8); // x=2 >= 0, so sub(50,8) = 42
    if (fptr_res2 != 42) return 13;

    return 42;
}
