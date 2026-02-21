// Test multi-character constants and constant expressions in array sizes
// EXPECT: 42

int main(void) {
    // Multi-character constant (GCC-compatible big-endian packing)
    int mc = 'AB';  // Should be ('A' << 8) | 'B' = (65 << 8) | 66 = 16706
    
    // Constant expression in array size
    int arr[2 + 3];  // Array of 5 elements
    arr[0] = 10;
    arr[1] = 20;
    arr[2] = 30;
    arr[3] = 40;
    arr[4] = 50;
    int sum = arr[0] + arr[1] + arr[2] + arr[3] + arr[4]; // 150
    
    // sizeof in array size
    int arr2[sizeof(int)];  // Array of 4 elements
    arr2[0] = 1;
    arr2[1] = 2;
    arr2[2] = 3;
    arr2[3] = 4;
    int sum2 = arr2[0] + arr2[1] + arr2[2] + arr2[3]; // 10
    
    // Shift in array size 
    int arr3[1 << 2];  // Array of 4 elements
    arr3[0] = 5;
    arr3[1] = 6;
    arr3[2] = 7;
    arr3[3] = 8;
    int sum3 = arr3[0] + arr3[1] + arr3[2] + arr3[3]; // 26
    
    // Multi-char constant verification: 'AB' mod 256 = 66 ('B')
    int mc_low = mc % 256;  // Should be 66
    
    // sum(150) + sum2(10) + sum3(26) - 150 - 10 - 26 + mc_low(66) - 66 + 42 = 42
    return sum - 150 + sum2 - 10 + sum3 - 26 + mc_low - 66 + 42;
}
