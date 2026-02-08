#define MAGIC 42

int main() {
    int x = 10;
    int *p = &x;
    
    // Test pointer arithmetic scaling
    // x is at some address, p points to it.
    // p + 1 should advance by sizeof(int) = 4 bytes.
    int *q = p + 1;
    
    // Test macro expansion (via gcc -E)
    if (MAGIC != 42) return 1;
    
    // Test optimization (DCE)
    int y = 50; // This variable is unused and should be removed by DCE
    int z = 100;
    z = z + 1; // z is used in return
    
    return z; // should return 101
}
