// Check if storing q works
// EXPECT: 1
int main() {
    int arr[2];
    arr[0] = 10;
    arr[1] = 20;
    
    int *p = arr;
    int *q;
    q = p + 1;
    
    // Check if q is non-null  if (q == 0) return 0;
    return 1;
}
