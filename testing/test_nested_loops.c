// EXPECT: 42
// Test nested loops: nested for, break/continue in nested context, loop accumulation
int main() {
    // Nested for loops — multiplication table accumulation
    int sum = 0;
    for (int i = 1; i <= 3; i++) {
        for (int j = 1; j <= 3; j++) {
            sum = sum + i * j;
        }
    }
    // (1*1+1*2+1*3)+(2*1+2*2+2*3)+(3*1+3*2+3*3) = 6+12+18 = 36
    if (sum != 36) return 1;

    // Break from inner loop only
    int found_i = -1, found_j = -1;
    for (int i = 0; i < 5; i++) {
        for (int j = 0; j < 5; j++) {
            if (i * 5 + j == 13) {
                found_i = i; // 2
                found_j = j; // 3
                break; // breaks inner only
            }
        }
        if (found_i >= 0) break; // breaks outer
    }
    if (found_i != 2) return 2;
    if (found_j != 3) return 3;

    // Continue in nested loop
    int count = 0;
    for (int i = 0; i < 4; i++) {
        if (i == 2) continue; // skip i=2
        for (int j = 0; j < 3; j++) {
            if (j == 1) continue; // skip j=1
            count++;
        }
    }
    // i in {0,1,3}, j in {0,2} → 3 * 2 = 6
    if (count != 6) return 4;

    // Triple-nested loop
    int vol = 0;
    for (int x = 0; x < 3; x++) {
        for (int y = 0; y < 3; y++) {
            for (int z = 0; z < 3; z++) {
                vol++;
            }
        }
    }
    if (vol != 27) return 5;

    return 42;
}
