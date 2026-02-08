// EXPECT: 20
// Test pointer arithmetic result
int main() {
    int arr[3];
    arr[0] = 10;
    arr[1] = 20;
    arr[2] = 30;
    
    int *p = arr;      // p points to arr[0]
    int val = *(p + 1); // Should dereference arr[1] = 20
    return val;
}
