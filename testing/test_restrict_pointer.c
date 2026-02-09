// EXPECT: 15
// Test restrict qualifier on pointers
int main() {
    int x = 10;
    int y = 5;
    int * restrict p = &x;
    int * restrict q = &y;
    
    *p = 15;
    // restrict tells compiler p and q don't alias
    return *p;
}
