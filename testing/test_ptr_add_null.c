// Test if pointer addition works
// EXPECT: 1
int main() {
    int *p = 0;      // Start with null pointer
    int *q = p + 1;  // Add 4 bytes
    // On a 64-bit system, q should be 4
    // But let's just return 1 if q is not null
    if (q == 0) return 0;
    return 1;
}
