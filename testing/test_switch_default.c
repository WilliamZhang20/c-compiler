// Test switch with default case
// EXPECT: 99
int main() {
    int x = 10;
    int result = 0;
    
    switch (x) {
        case 1:
            result = 10;
            break;
        case 2:
            result = 20;
            break;
        default:
            result = 99;
            break;
    }
    
    return result;  // Should return 99
}
