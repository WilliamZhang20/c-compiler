// Test with a known pointer value
// EXPECT: 4
int main() {
    int *p = (int*)0;  // Cast 0 to pointer
    int *q = p + 1;
    // q should be 4 (since sizeof(int) = 4)
    return (int)q;  // This will be 4 on x64, might not work on all platforms but let's try
}
