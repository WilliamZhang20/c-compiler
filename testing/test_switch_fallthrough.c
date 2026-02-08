// Test switch with fallthrough
// EXPECT: 5
int main() {
    int x = 2;
    int result = 0;
    
    switch (x) {
        case 1:
            result = result + 1;
        case 2:
            result = result + 2;
        case 3:
            result = result + 3;
            break;
        default:
            result = 99;
    }
    
    return result;  // Should return 5 (2 + 3 due to fallthrough)
}
