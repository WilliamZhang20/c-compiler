// Simple function pointer test
// EXPECT: 15
int add(int a, int b) {
    return a + b;
}

int main() {
    int (*fp)(int, int);
    fp = add;
    return fp(10, 5);
}
