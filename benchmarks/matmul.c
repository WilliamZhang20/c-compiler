// Matrix multiply simplified - using 1D arrays
// Tests nested loops and array indexing
int main() {
    int a[100];
    int b[100];
    int c[100];
    int i;
    int j;
    int k;
    int N = 10;
    
    // Initialize matrices (stored as 1D arrays, 10x10)
    for (i = 0; i < 100; i = i + 1) {
        a[i] = i % N;
        b[i] = (i / N) % N;
        c[i] = 0;
    }
    
    // Matrix multiplication (simplified)
    for (i = 0; i < N; i = i + 1) {
        for (j = 0; j < N; j = j + 1) {
            for (k = 0; k < N; k = k + 1) {
                c[i * N + j] = c[i * N + j] + a[i * N + k] * b[k * N + j];
            }
        }
    }
    
    return c[55] % 256;  // Element at [5][5]
}
