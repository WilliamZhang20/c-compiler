// EXPECT: 42
// Test const qualifier - value should not be modifiable
int main() {
    const int x = 42;
    // Trying to modify x would fail semantic analysis
    return x;
}
