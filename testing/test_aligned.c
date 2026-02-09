// EXPECT: 42
// Test __attribute__((aligned(N))) on globals
// The aligned attribute ensures variables are aligned to specified byte boundaries

int __attribute__((aligned(16))) aligned_var = 42;

int main() {
    return aligned_var;
}
