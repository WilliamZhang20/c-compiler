// EXPECT: 55
// Test volatile qualifier - marks variable as volatile
int main() {
    volatile int x = 55;
    // Volatile tells compiler not to optimize away accesses
    return x;
}
