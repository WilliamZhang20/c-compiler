// Comprehensive switch test with nested switches
// EXPECT: 123
int main() {
    int x = 2;
    int y = 3;
    int result = 0;
    
    // Outer switch
    switch (x) {
        case 1:
            result = 100;
            break;
        case 2:
            // Nested switch
            switch (y) {
                case 1:
                    result = 110;
                    break;
                case 2:
                    result = 120;
                    break;
                case 3:
                    result = 123;
                    break;
                default:
                    result = 199;
            }
            break;
        case 3:
            result = 300;
            break;
        default:
            result = 999;
    }
    
    return result;
}
