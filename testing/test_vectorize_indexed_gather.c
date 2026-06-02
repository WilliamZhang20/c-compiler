// EXPECT: 52
// Indirect gather: a[idx[i]] = b[idx[i]] with idx[i]=i+1
int main() {
    int a[64];
    int b[64];
    int idx[64];
    int i;
    for (i = 0; i < 64; i = i + 1) {
        idx[i] = i + 1;
    }
    for (i = 0; i < 64; i = i + 1) {
        b[i] = i + 10;
    }
    for (i = 0; i < 64; i = i + 1) {
        a[idx[i]] = b[idx[i]];
    }
    return a[42];
}
