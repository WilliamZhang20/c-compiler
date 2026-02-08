// Test with a known pointer value
// EXPECT: 4
int main() {
    int *p = (int*)0;  // Cast 0 to pointer
    int *q = p + 1;
    // q should be 4 (since sizeof(int) = 4 in our implementation)
    return (int)q;
}
