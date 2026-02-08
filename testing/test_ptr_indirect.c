// Test pointer from array + arithmetic
// EXPECT: 44
int main() {
    int arr[2];
    arr[0] = 33;
    arr[1] = 44;
    
    int *p = arr;
    int **pp = &p;  // Get address of p
    int *q = *pp;   // Load p's value through pp
    q = q + 1;      // Add 1 to the pointer
    return *q;
}
