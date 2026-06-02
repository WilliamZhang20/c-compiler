// Fibonacci — iterative O(n) loop (same algorithm in source for all compilers).
// Measures loop + integer codegen, not automatic recurrence elimination.
int fib(int n) {
    int i;
    int prev;
    int curr;
    int next;

    if (n <= 1) {
        return n;
    }

    prev = 0;
    curr = 1;
    for (i = 2; i <= n; i = i + 1) {
        next = prev + curr;
        prev = curr;
        curr = next;
    }
    return curr;
}

int main() {
    return fib(35) % 256;
}
