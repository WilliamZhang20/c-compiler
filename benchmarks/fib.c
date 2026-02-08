// Fibonacci - tests function calls and recursion optimization potential
int fib(int n) {
    if (n <= 1) return n;
    return fib(n - 1) + fib(n - 2);
}

int main() {
    return fib(20);  // Returns 6765
}
