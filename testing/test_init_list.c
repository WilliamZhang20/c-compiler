// EXPECT: 0
// Test brace-enclosed initializer lists for arrays and structs

struct Point {
    int x;
    int y;
};

int main() {
    // 1. Basic array initializer list
    int arr[5] = {10, 20, 30, 40, 50};
    if (arr[0] != 10) return 1;
    if (arr[1] != 20) return 2;
    if (arr[2] != 30) return 3;
    if (arr[3] != 40) return 4;
    if (arr[4] != 50) return 5;

    // 2. Sum the array
    int sum = arr[0] + arr[1] + arr[2] + arr[3] + arr[4];
    if (sum != 150) return 6;

    // 3. Array with inferred size
    int arr2[] = {1, 2, 3};
    if (arr2[0] != 1) return 7;
    if (arr2[1] != 2) return 8;
    if (arr2[2] != 3) return 9;

    // 4. Struct initializer list
    struct Point p = {5, 8};
    if (p.x != 5) return 10;
    if (p.y != 8) return 11;

    // 5. Designated struct initializer
    struct Point q = {.x = 100, .y = 200};
    if (q.x != 100) return 12;
    if (q.y != 200) return 13;

    // 6. Designated array initializer
    int darr[4] = {[0] = 11, [2] = 33};
    if (darr[0] != 11) return 14;
    if (darr[2] != 33) return 15;

    return 0;
}
