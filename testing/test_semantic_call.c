// EXPECT: 30
// Prototype enables call type checking at compile time
int add(int a, int b);

int add(int a, int b) {
    return a + b;
}

int main(void) {
    return add(10, 20);
}
