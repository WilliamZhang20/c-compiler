// Test integer literal suffixes (U, L, UL, LL, ULL)
// EXPECT: 42

int main(void) {
    int a = 10U;
    int b = 20L;
    int c = 5UL;
    int d = 7ULL;
    // 10 + 20 + 5 + 7 = 42
    return a + b + c + d;
}
