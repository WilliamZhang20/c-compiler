// Ultra simple: does p+1 actually change p?
// EXPECT: 1
int main() {
    int arr[2];
    arr[0] = 10;
    arr[1] = 20;
    
    int *p = arr;
    int *q = p + 1;
    
    // Check if q != p
    if (q == p) return 0;
    return 1;
}
