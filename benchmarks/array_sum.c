// Array sum - tests loop optimization and memory access patterns
int main() {
    int arr[1000];
    int i;
    int sum = 0;
    
    // Initialize array
    for (i = 0; i < 1000; i = i + 1) {
        arr[i] = i;
    }
    
    // Sum array elements
    for (i = 0; i < 1000; i = i + 1) {
        sum = sum + arr[i];
    }
    
    return sum % 256;  // Returns 116
}
