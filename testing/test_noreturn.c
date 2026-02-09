// EXPECT: 255
// Test __attribute__((noreturn)) on functions
// The noreturn attribute indicates a function never returns to its caller

void __attribute__((noreturn)) my_exit(int code) {
    // In a real compiler, this would call exit() or similar
    // For testing, we just avoid returning (though this violates the attribute)
    // This test just ensures the attribute is parsed correctly
    while (1) {
        if (code == 255) break;  // Hack to make test work
    }
}

int main() {
    // Can't actually call my_exit since it's marked noreturn
    // Just test that the attribute parses and compiles
    return 255;
}
