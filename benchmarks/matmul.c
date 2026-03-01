// Matrix multiply - using 2D arrays
// Tests nested loops, 2D array indexing, and loop optimization
int main() {
    int a[100][100];
    int b[100][100];
    int c[100][100];
    int i;
    int j;
    int k;
    int rep;
    int checksum = 0;

    // Initialize matrices
    for (i = 0; i < 100; i = i + 1) {
        for (j = 0; j < 100; j = j + 1) {
            a[i][j] = (i * 7 + j * 3) % 100;
            b[i][j] = (i * 5 + j * 11) % 100;
        }
    }

    // Repeat matrix multiplication to get measurable runtime
    for (rep = 0; rep < 20; rep = rep + 1) {
        // Zero c
        for (i = 0; i < 100; i = i + 1) {
            for (j = 0; j < 100; j = j + 1) {
                c[i][j] = 0;
            }
        }
        // Matrix multiplication
        for (i = 0; i < 100; i = i + 1) {
            for (j = 0; j < 100; j = j + 1) {
                for (k = 0; k < 100; k = k + 1) {
                    c[i][j] = c[i][j] + a[i][k] * b[k][j];
                }
            }
        }
        checksum = checksum + c[rep % 100][rep % 100];
    }

    return checksum % 256;
}
