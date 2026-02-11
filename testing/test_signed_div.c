// EXPECT: 0
int main() {
    int a = -7;
    int b = 4;
    int div = a / b; // Should be -1, not -2 (if optimization was wrong)
    int mod = a % b; // Should be -3, not 1
    
    if (div != -1) return 1;
    if (mod != -3) return 2;
    return 0;
}
