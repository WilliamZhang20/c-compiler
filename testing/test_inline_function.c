// EXPECT: 30
// Test inline function
inline int add(int a, int b) {
    return a + b;
}

int main() {
    return add(10, 20);
}
