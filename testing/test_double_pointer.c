// EXPECT: 42
// Test double pointers (int **), pointer-to-pointer operations
int main() {
    int a = 10;
    int b = 32;
    int *pa = &a;
    int *pb = &b;
    int **ppa = &pa;

    // Double dereference
    int val = **ppa; // 10
    if (val != 10) return 1;

    // Modify through double pointer
    **ppa = 20;
    if (a != 20) return 2;

    // Change what the pointer points to through double pointer
    *ppa = pb;
    if (**ppa != 32) return 3;

    // Array of pointers
    int x = 5, y = 15, z = 22;
    int *ptrs[3];
    ptrs[0] = &x;
    ptrs[1] = &y;
    ptrs[2] = &z;

    int sum = 0;
    for (int i = 0; i < 3; i++) {
        sum = sum + *ptrs[i];
    }
    // sum = 5 + 15 + 22 = 42
    if (sum != 42) return 4;

    return sum; // 42
}
