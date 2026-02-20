// EXPECT: 42
// Test function parameters are correctly preserved across function calls
int add(int a, int b) {
    // Call printf which uses rdi/rsi, potentially clobbering parameters
    printf("%d + %d = %d\n", a, b, a + b);
    return a + b;
}

int main() {
    return add(20, 22);
}
