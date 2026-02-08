// Check the arithmetic value directly
// EXPECT: 2
int main() {
    int arr[2];
    arr[0] = 10;
    arr[1] = 20;
    
    int *p = arr;
    int *q = p + 1;
    
    // Calculate difference in bytes  
    int diff = (int)q - (int)p;
    
    // Should be 8 for sizeof(int) in our implementation
    if (diff == 8) return 1;
    if (diff == 4) return 2;  // If using 4-byte ints
    if (diff == 1) return 3;  // If scaled by 1 instead of size
    return 0;
}
