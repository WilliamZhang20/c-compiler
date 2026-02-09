// EXPECT: 42
// Test basic goto statement

int main() {
    int x = 0;
    goto skip;
    x = 100;  // This should be skipped
skip:
    x = 42;
    return x;
}
