// Fibonacci - tests function calls and recursion optimization potential
int fib(int n) {
    if (n <= 1) return n;
    return fib(n - 1) + fib(n - 2);
}

int main() {
    int total = 0;
    int i;
    // fib(35) = 9227465, about 9M recursive calls
    total = fib(35);
    return total % 256;
}
