// EXPECT: 64
// Loop-carried shift: a[i+1] = a[i] + 1 must stay scalar (SIMD would break deps).
int main() {
    int a[64];
    int i;
    a[0] = 1;
    for (i = 0; i < 63; i = i + 1) {
        a[i + 1] = a[i] + 1;
    }
    return a[63];
}
