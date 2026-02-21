// EXPECT: 0
// Test: matrix multiply with triple-nested loops and 2D arrays.
// Exercises mem2reg phi simplification across multiple loop nesting levels,
// where simplified phi VarIds appear in GEP operands (not just other phis).
int main() {
    int a[4][4];
    int b[4][4];
    int c[4][4];
    int i;
    int j;
    int k;
    int n = 4;

    // Initialize a = identity, b = [1..16]
    for (i = 0; i < n; i = i + 1) {
        for (j = 0; j < n; j = j + 1) {
            if (i == j)
                a[i][j] = 1;
            else
                a[i][j] = 0;
            b[i][j] = i * n + j + 1;
            c[i][j] = 0;
        }
    }

    // c = a * b (identity * b should equal b)
    for (i = 0; i < n; i = i + 1) {
        for (j = 0; j < n; j = j + 1) {
            for (k = 0; k < n; k = k + 1) {
                c[i][j] = c[i][j] + a[i][k] * b[k][j];
            }
        }
    }

    // Verify c == b
    for (i = 0; i < n; i = i + 1) {
        for (j = 0; j < n; j = j + 1) {
            if (c[i][j] != b[i][j])
                return 1;
        }
    }

    return 0;
}
