// EXPECT: 42
// Test sizeof operator with various types and expressions
struct S { int a; char b; int c; };
struct Packed { char a; char b; };

int main() {
    // sizeof basic types
    if (sizeof(char) != 1) return 1;
    if (sizeof(int) != 4) return 2;
    if (sizeof(long) != 8) return 3;
    if (sizeof(long long) != 8) return 4;

    // sizeof pointer (8 bytes on x86-64)
    int *p = (int*)0;
    if (sizeof(p) != 8) return 5;
    if (sizeof(int*) != 8) return 6;
    if (sizeof(char*) != 8) return 7;

    // sizeof array
    int arr[10];
    if (sizeof(arr) != 40) return 8; // 10 * 4

    char str[20];
    if (sizeof(str) != 20) return 9;

    // sizeof struct (with padding)
    if (sizeof(struct S) < 9) return 10; // at least 4+1+4=9, likely 12 with padding

    // sizeof expression (doesn't evaluate the expression)
    int x = 5;
    int s = sizeof(x + 1); // sizeof(int) = 4, x is NOT modified
    if (s != 4) return 11;

    // sizeof applied to dereferenced pointer
    int val = 42;
    int *ptr = &val;
    if (sizeof(*ptr) != 4) return 12;

    // sizeof float/double
    if (sizeof(float) != 4) return 13;
    if (sizeof(double) != 8) return 14;

    return 42;
}
