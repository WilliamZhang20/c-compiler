// EXPECT: 42
// Test void pointer casting and generic pointer operations
int main() {
    int x = 42;
    // Cast to void* and back
    void *vp = (void*)&x;
    int *ip = (int*)vp;
    if (*ip != 42) return 1;

    // Void pointer to different types
    char c = 'A'; // 65
    void *vc = (void*)&c;
    char *cp = (char*)vc;
    if (*cp != 65) return 2;

    // Void pointer arithmetic (cast to char* for byte-level)
    int arr[3];
    arr[0] = 100;
    arr[1] = 200;
    arr[2] = 300;

    void *base = (void*)arr;
    // Access second element via char* cast + offset
    int *p1 = (int*)((char*)base + 4); // sizeof(int) = 4
    if (*p1 != 200) return 3;

    // Null pointer check
    void *null_ptr = (void*)0;
    if (null_ptr) return 4; // should be false

    void *non_null = (void*)&x;
    if (!non_null) return 5; // should be true

    // Multiple casts
    long l = 999;
    void *vl = (void*)&l;
    long *lp = (long*)vl;
    if (*lp != 999) return 6;

    return *ip; // 42
}
