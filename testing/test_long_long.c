// EXPECT: 42
// Test long long type: arithmetic, comparisons, shifts
int main() {
    long long a = 1000000000LL;
    long long b = 2000000000LL;
    long long c = a + b; // 3000000000 (exceeds int range)

    // Verify it exceeds 32-bit: 3000000000 > 2^31-1 = 2147483647
    if (c <= 2147483647LL) return 1;

    // Long long arithmetic
    long long d = c - a;
    if (d != 2000000000LL) return 2;

    long long e = 100LL * 100LL; // 10000
    if (e != 10000LL) return 3;

    // Long long division
    long long f = 1000000LL / 1000LL;
    if (f != 1000LL) return 4;

    // Long long shifts
    long long g = 1LL << 32; // 4294967296 — exceeds 32 bits
    if (g <= 0) return 5;

    long long h = g >> 16; // 65536
    if (h != 65536LL) return 6;

    // Comparison
    long long big = 5000000000LL;
    long long small = 100LL;
    if (big <= small) return 7;
    if (small >= big) return 8;

    // Cast to int (truncation)
    int truncated = (int)(e % 256LL); // 10000 % 256 = 16
    if (truncated != 16) return 9;

    // Unsigned long long
    unsigned long long u = 18446744073709551615ULL; // ULLONG_MAX
    unsigned long long u2 = u - 1ULL;
    if (u2 >= u) return 10;

    return 42;
}
