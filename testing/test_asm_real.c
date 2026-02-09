// EXPECT: 42
// Test actual inline assembly

int main() {
    int result = 0;
    // Use inline assembly to set result to 42 (Intel syntax: dest, src)
    asm volatile ("mov %0, 42" : "=r"(result));
    return result;
}
