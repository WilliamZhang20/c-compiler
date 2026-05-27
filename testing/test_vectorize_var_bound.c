// EXPECT: 0
// Vectorize: runtime loop bound (variable)
int sum_array(int *p, int n) {
    int i;
    int s = 0;
    for (i = 0; i < n; i = i + 1) {
        s = s + p[i];
    }
    return s;
}

int main() {
    int arr[32];
    int i;
    int n = 32;
    for (i = 0; i < 32; i = i + 1) {
        arr[i] = i;
    }
    return sum_array(arr, n) - 496;
}
