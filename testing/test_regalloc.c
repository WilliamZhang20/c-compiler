// Test register allocation with many local variables
// EXPECT: 210
int main() {
    int x1 = 10;
    int x2 = 20;
    int x3 = 30;
    int x4 = 40;
    int x5 = 50;
    int x6 = 60;
    
    // This should benefit from register allocation
    int sum1 = x1 + x2;  // 30
    int sum2 = x3 + x4;  // 70
    int sum3 = x5 + x6;  // 110
    
    int result = sum1 + sum2 + sum3;  // 210
    return result;
}
