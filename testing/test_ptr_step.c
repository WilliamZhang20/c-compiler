// Minimal test: Store array pointer, read it, do arithmetic
// EXPECT: 66
int main() {
    int arr[2];
    arr[0] = 55;
    arr[1] = 66;
    
    int *p;
    p = arr;        // Assign array to pointer
    int val = *p;   // Should be 55
    p = p + 1;      // Advance pointer
    return *p;      // Should be 66
}
