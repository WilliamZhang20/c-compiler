// EXPECT: 0
// Strided gather/scatter: a[2*i] = b[2*i], trip 16 (vectorizable at vf=8)
int main() {
    int a[32];
    int b[32];
    int i;
    for (i = 0; i < 16; i = i + 1) {
        b[i * 2] = i;
    }
    for (i = 0; i < 16; i = i + 1) {
        a[i * 2] = b[i * 2];
    }
    return a[16] - b[16];
}
