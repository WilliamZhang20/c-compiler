// Bitwise operations - popcount hot loop (matches __builtin_popcount in source)
int main() {
    int i;
    int rep;
    int total = 0;

    for (rep = 0; rep < 2000; rep = rep + 1) {
        for (i = 0; i < 10000; i = i + 1) {
            total = total + __builtin_popcount((unsigned)i);
        }
    }

    return total % 256;
}
