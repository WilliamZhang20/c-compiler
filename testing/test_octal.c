int printf(const char *fmt, ...);

int main() {
    int perms = 0644;
    int mask = 0777;
    int small = 01;
    int zero = 0;
    
    // 0644 octal = 420 decimal
    // 0777 octal = 511 decimal
    printf("%d %d %d %d\n", perms, mask, small, zero);
    
    if (perms != 420) return 1;
    if (mask != 511) return 2;
    if (small != 1) return 3;
    if (zero != 0) return 4;
    
    return 0;
}
