// EXPECT: 42
// Test array indexing only
int main() {
    int arr[3];
    arr[0] = 10;
    arr[1] = 42;
    arr[2] = 30;
    
    return arr[1];
}
