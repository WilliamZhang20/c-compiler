// EXPECT: 42
// Simple pointer arithmetic test
int main() {
    int arr[3];
    arr[0] = 10;
    arr[1] = 42;
    arr[2] = 30;
    
    int *p = arr;      // p points to arr[0]
    int *q = p + 1;    // q should point to arr[1]
    return *q;         // Should return 42
}
