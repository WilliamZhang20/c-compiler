// Test inline pointer arithmetic with dereference
// EXPECT: 88
int main() {
    int arr[2];
    arr[0] = 77;
    arr[1] = 88;
    
    int *p = arr;
    return p[1];  // Array subscript notation is same as *(p+1)
}
