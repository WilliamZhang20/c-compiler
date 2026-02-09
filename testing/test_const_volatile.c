// EXPECT: 99
// Test combining const and volatile qualifiers
int main() {
    const volatile int x = 99;
    // const volatile is used for memory-mapped I/O
    return x;
}
