// EXPECT: 72
// Test char array initialization with string literal
int main() {
    char str[] = "Hi";  // Should infer size 3 (H, i, \0)
    return str[0];  // Return 'H' = 72
}
