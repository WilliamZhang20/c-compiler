// Test with a known pointer value
// EXPECT: 8
int main() {
    int *p = (int*)0;  // Cast 0 to pointer
    int *q = p + 1;
    // q should be 8 (since sizeof(int) = 8 in our implementation)
    return (int)q;
}
