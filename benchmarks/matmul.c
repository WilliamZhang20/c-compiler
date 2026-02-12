// Matrix multiply - using 2D arrays
// Tests nested loops and 2D array indexing
int main() {
    int a[10][10];
    int b[10][10];
    int c[10][10];
    int i; 
    int j;
    int k;
    int N = 10;
    
    // Initialize matrices
    for (i = 0; i < 10; i = i + 1) {
        for (j = 0; j < 10; j = j + 1) {
            a[i][j] = i % N;
            b[i][j] = (i / N) % N;
            c[i][j] = 0;
        }
    }
    
    // Matrix multiplication
    for (i = 0; i < 10; i = i + 1) {
        for (j = 0; j < 10; j = j + 1) {
            for (k = 0; k < 10; k = k + 1) {
                c[i][j] = c[i][j] + a[i][k] * b[k][j];
            }
        }
    }
    
    return c[5][5] % 256; 
}
