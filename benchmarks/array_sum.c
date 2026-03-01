// Array sum - tests loop optimization and memory access patterns
int main() {
    int arr[10000];
    int i;
    int rep;
    int sum = 0;

    // Initialize array
    for (i = 0; i < 10000; i = i + 1) {
        arr[i] = i % 256;
    }

    // Repeat summing to get measurable runtime
    for (rep = 0; rep < 5000; rep = rep + 1) {
        int s = 0;
        for (i = 0; i < 10000; i = i + 1) {
            s = s + arr[i];
        }
        sum = sum + s;
    }

    return sum % 256;
}
