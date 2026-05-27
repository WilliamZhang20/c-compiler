// EXPECT: 1
// Trip count 10 with VF=4/8: last 2 or 6 iterations use masked vector tail, not scalar peel.
int main() {
    int a[16];
    int b[16];
    int i;
    for (i = 0; i < 10; i = i + 1) {
        b[i] = 1;
    }
    for (i = 0; i < 10; i = i + 1) {
        a[i] = b[i];
    }
    return a[9];
}
