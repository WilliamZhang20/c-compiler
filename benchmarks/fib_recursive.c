// Recursive fibonacci — used to test recurrence elimination (not in headline benchmark suite).
// With GCC -O3 this becomes a hybrid IPO loop; with our compiler it becomes O(n) iteration.
int fib(int n) {
    if (n <= 1) return n;
    return fib(n - 1) + fib(n - 2);
}

int main() {
    return fib(35) % 256;
}
