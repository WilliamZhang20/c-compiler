// Test int array indexing
// EXPECT: 90
int main() {
    int arr[5];
    arr[0] = 10;
    arr[1] = 20;
    arr[2] = 30;
    arr[3] = 40;
    arr[4] = 50;
    
    int sum = arr[0] + arr[2] + arr[4];
    return sum;  // Should return 90
}
