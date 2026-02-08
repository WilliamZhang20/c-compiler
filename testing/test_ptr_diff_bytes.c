// Check the arithmetic value directly
// EXPECT: 1
int main() {
    int arr[2];
    arr[0] = 10;
    arr[1] = 20;
    
    int *p = arr;
    int *q = p + 1;
    
    // Calculate difference in bytes  
    int diff = (int)q - (int)p;
    
    // Should be 4 for sizeof(int)
    if (diff == 4) return 1;
    if (diff == 1) return 2;  // If scaled by 1 instead of 4
    return 0;
}
