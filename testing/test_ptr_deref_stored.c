// Final test: Can we deref the stored pointer?
// EXPECT: 20
int main() {
    int arr[2];
    arr[0] = 10;
    arr[1] = 20;
    
    int *p = arr;
    int *q;
    q = p + 1;
    
    return *q;
}
