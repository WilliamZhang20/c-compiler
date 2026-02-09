// EXPECT: 15
// Test __attribute__((always_inline)) on functions
// The always_inline attribute suggests the function should always be inlined

int __attribute__((always_inline)) add(int a, int b) {
    return a + b;
}

int main() {
    int result = add(10, 5);
    return result;
}
