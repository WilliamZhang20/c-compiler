// EXPECT: 0
// Test char array initializer list

int main() {
    // Char array with initializer list
    char hello[6] = {'H', 'e', 'l', 'l', 'o', 0};
    if (hello[0] != 72) return 1;  // 'H' = 72
    if (hello[1] != 101) return 2; // 'e' = 101
    if (hello[4] != 111) return 3; // 'o' = 111
    if (hello[5] != 0) return 4;   // null terminator

    // Int array with trailing comma (allowed in C)
    int nums[] = {100, 200, 300,};
    if (nums[0] != 100) return 5;
    if (nums[1] != 200) return 6;
    if (nums[2] != 300) return 7;

    return 0;
}
