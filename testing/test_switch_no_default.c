// Test switch without default (no match)
// EXPECT: 42
int main() {
    int x = 10;
    int result = 42;
    
    switch (x) {
        case 1:
            result = 10;
            break;
        case 2:
            result = 20;
            break;
    }
    
    return result;  // Should return 42 (unchanged)
}
