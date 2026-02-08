// Test direct arithmetic dereference without assignment
// EXPECT: 77
int main() {
    int arr[2];
    arr[0] = 66;
    arr[1] = 77;
    
    int *p = arr;
    return *(p + 1);  // Direct dereference of arithmetic result
}
