// Bitwise operations - tests optimization of bit manipulation
int popcount(int x) {
    int count = 0;
    int bit;
    for (bit = 0; bit < 32; bit = bit + 1) {
        if (x & (1 << bit)) {
            count = count + 1;
        }
    }
    return count;
}

int main() {
    int i;
    int rep;
    int total = 0;

    for (rep = 0; rep < 2000; rep = rep + 1) {
        for (i = 0; i < 10000; i = i + 1) {
            total = total + popcount(i);
        }
    }

    return total % 256;
}
