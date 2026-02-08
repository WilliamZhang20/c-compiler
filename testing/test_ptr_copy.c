// Test if loading pointer works
// EXPECT: 55
int main() {
    int x = 55;
    int *p = &x;
    int *q = p;    // Copy pointer
    return *q;
}
