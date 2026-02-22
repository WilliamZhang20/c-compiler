// EXPECT: 42
// Test while loops with various patterns including modulo and division
int main() {
    // Basic while
    int sum = 0;
    int i = 0;
    while (i < 10) {
        sum = sum + i;
        i++;
    }
    if (sum != 45) return 1;

    // Nested while loops
    int product = 0;
    int a = 0;
    while (a < 3) {
        int b = 0;
        while (b < 4) {
            product++;
            b++;
        }
        a++;
    }
    if (product != 12) return 2;

    // While with modulo and division (Collatz-style)
    int x = 84;
    int steps = 0;
    while (x > 1) {
        if (x % 2 == 0) {
            x = x / 2;
        } else {
            x = x - 1;
        }
        steps++;
    }
    // 84 -> 42 -> 21 -> 20 -> 10 -> 5 -> 4 -> 2 -> 1 (8 steps)
    if (x != 1) return 3;
    if (steps != 8) return 30;

    // While with early return
    int arr[5];
    arr[0] = 10; arr[1] = 20; arr[2] = 42; arr[3] = 50; arr[4] = 60;
    int idx = 0;
    while (idx < 5) {
        if (arr[idx] == 42) return 42;
        idx++;
    }

    return 0; // shouldn't reach here
}
