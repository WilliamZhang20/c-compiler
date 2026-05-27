// EXPECT: 1
// Disjoint arrays, unit stride — safe to vectorize.
int main() {
    int a[128];
    int b[128];
    int i;
    for (i = 0; i < 128; i = i + 1) {
        b[i] = 1;
    }
    for (i = 0; i < 128; i = i + 1) {
        a[i] = b[i];
    }
    return a[100];
}
