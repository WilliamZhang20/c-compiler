// Simple test for register allocation and strength reduction
// EXPECT: 80
int main() {
    int a = 10;
    int b = a * 8;  // Should become shift left by 3
    return b;  // Expected: 80
}
