// Check if p is being loaded correctly
// EXPECT: 99
int main() {
    int arr[2];
    arr[0] = 99;
    arr[1] = 88;
    
    int *p = arr;
    return *p;  // Should be 99
}
