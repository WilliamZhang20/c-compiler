// Test just loading pointer value
// EXPECT: 1
int main() {
    int x = 100;
    int *p = &x;
    
    // Check if p is non-null
    if (p == 0) return 0;
    return 1;
}
