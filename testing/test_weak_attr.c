// Test __attribute__((weak)) - weak function definition
// EXPECT: 42

// Weak function - can be overridden at link time
__attribute__((weak)) int get_value(void) {
    return 42;
}

int main(void) {
    // Since no strong definition overrides, this uses the weak one
    return get_value();
}
