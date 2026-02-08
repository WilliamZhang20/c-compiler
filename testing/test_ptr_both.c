// Check if p points to arr properly
// EXPECT: 20
int main() {
    int arr[2];
    arr[0] = 10;
    arr[1] = 20;
    
    int *p = arr;
    int *q = p + 1;
    
    // First check if p works
    int val_p = *p;
    if (val_p != 10) return 99;  // p is wrong
    
    // Now check q
    int val_q = *q;
    return val_q;  // Should be 20
}
