// Test function pointers
// EXPECT: 42
int add(int a, int b) {
    return a + b;
}

int multiply(int a, int b) {
    return a * b;
}

int main() {
    int (*op)(int, int);
    
    op = add;
    int result1 = op(10, 5);  // 15
    
    op = multiply;
    int result2 = op(3, 9);   // 27
    
    return result1 + result2; // 42
}
