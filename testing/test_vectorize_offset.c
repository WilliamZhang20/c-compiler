// EXPECT: 42
// Vectorize: a[i+1] = b[i+1] with constant trip count
int main() {
    int a[65];
    int b[65];
    int i;
    for (i = 0; i < 64; i = i + 1) {
        b[i + 1] = i + 1;
    }
    for (i = 0; i < 64; i = i + 1) {
        a[i + 1] = b[i + 1];
    }
    return a[42];
}
