// Benchmark: stride-2 index pattern a[2*i] = b[2*i]
int main() {
    int a[2000];
    int b[2000];
    int i;
    for (i = 0; i < 1000; i = i + 1) {
        a[i * 2] = b[i * 2];
    }
    return a[1998] - b[1998];
}
