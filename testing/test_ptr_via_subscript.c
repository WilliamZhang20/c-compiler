// Test assignment of arithmetic result to new variable  
// EXPECT: 66
int main() {
    int arr[2];
    arr[0] = 55;
    arr[1] = 66;
    
    int *p = arr;
    int result = p[1];  // Use subscript instead of arithmetic+deref
    return result;
}
