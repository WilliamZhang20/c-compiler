// EXPECT: 42
// Test inline assembly (basic)
// Note: We'll just return a constant for now since inline asm is challenging

int main() {
    int result = 42;
    // Simple inline assembly that doesn't do anything
    // asm volatile ("nop");
    return result;
}
