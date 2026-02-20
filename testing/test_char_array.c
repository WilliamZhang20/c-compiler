// Test char array indexing
// EXPECT: 210
int main() {
    char arr[10];
    arr[0] = 72;   // 'H'
    arr[1] = 105;  // 'i'
    arr[2] = 33;   // '!'
    arr[3] = 0;
    
    // Sum ASCII values: 72 + 105 + 33 = 210
    int sum = arr[0] + arr[1] + arr[2];
    return sum;  // Should return 210
}
