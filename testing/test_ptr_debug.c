// Debug: Check what p + 1 evaluates to
// EXPECT: 71
int main() {
    int arr[3];
    arr[0] = 10;
    arr[1] = 20;
    arr[2] = 71;
    
    int *p = arr;        // p = address of arr[0]
    int val1 = *(p + 2); // Should be arr[2] = 71
    return val1;
}
