// EXPECT: 20
// Simplified test of qualifiers
const int CONST_GLOBAL = 10;

int multiply(int a, int b) {
    return a * b;
}

int main() {
    return multiply(CONST_GLOBAL, 2);  // 10 * 2 = 20
}
