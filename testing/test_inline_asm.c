// EXPECT: 42
// Test inline assembly (basic)

int main() {
    int result = 0;
    // Inline assembly to set result to 42 (Intel syntax: dest, src)
    __asm volatile ("mov %0, 42" : "=r"(result));
    return result;
}
