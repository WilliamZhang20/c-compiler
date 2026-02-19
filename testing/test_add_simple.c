// EXPECT: 27
int add(int a, int b) {
    return a + b;
}

int main() {
    int x = add(10, 5);
    int y = add(3, 9);
    return x + y;
}
