// EXPECT: 9
// Bitwise loop (scalar path; SIMD And covered by optimizer unit tests)
int main() {
    int a[8];
    int b[8];
    int c[8];
    int i;
    for (i = 0; i < 8; i = i + 1) {
        a[i] = 7;
        b[i] = 3;
    }
    for (i = 0; i < 3; i = i + 1) {
        c[i] = a[i] & b[i];
    }
    return c[0] + c[1] + c[2];
}
