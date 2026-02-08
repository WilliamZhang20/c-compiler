// Check uninitialized var assignment
// EXPECT: 42
int main() {
    int x = 42;
    int *p;
    p = &x;
    return *p;
}
