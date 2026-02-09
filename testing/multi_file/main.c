// EXPECT: 42
// Main file for multi-file test

// Forward declarations
int add(int a, int b);
int multiply(int a, int b);

int main() {
    int x = add(10, 5);        // 15
    int y = multiply(3, 9);     // 27
    return x + y;               // 42
}
