// EXPECT: 42
// Test pointer to array
int main() {
    int arr[3];
    arr[0] = 10;
    arr[1] = 42;
    arr[2] = 30;
    
    int *p = arr;      // p points to arr[0]
    return *p + 32;    // Should return 42
}
