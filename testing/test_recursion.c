// EXPECT: 89
// Test recursive functions: factorial, fibonacci, mutual recursion
int fibonacci(int n) {
    if (n <= 1) return n;
    return fibonacci(n - 1) + fibonacci(n - 2);
}

int factorial(int n) {
    if (n <= 1) return 1;
    return n * factorial(n - 1);
}

// Mutual recursion
int is_odd(int n);
int is_even(int n) {
    if (n == 0) return 1;
    return is_odd(n - 1);
}
int is_odd(int n) {
    if (n == 0) return 0;
    return is_even(n - 1);
}

int main() {
    int fib11 = fibonacci(11); // 89

    int fact5 = factorial(5); // 120
    if (fact5 != 120) return 1;

    // Mutual recursion tests
    if (!is_even(0)) return 2;
    if (is_odd(0)) return 3;
    if (!is_even(4)) return 4;
    if (!is_odd(7)) return 5;
    if (is_even(3)) return 6;

    return fib11; // 89
}
