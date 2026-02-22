// EXPECT: 42
// Test logical short-circuit evaluation: && and ||
int side_effect_count;

int inc_and_return(int val) {
    side_effect_count++;
    return val;
}

int main() {
    // && short-circuit: second operand NOT evaluated if first is false
    side_effect_count = 0;
    int r1 = 0 && inc_and_return(1);
    if (side_effect_count != 0) return 1; // must NOT have called inc_and_return
    if (r1 != 0) return 2;

    // && evaluates both when first is true
    side_effect_count = 0;
    int r2 = 1 && inc_and_return(1);
    if (side_effect_count != 1) return 3;
    if (r2 != 1) return 4;

    // || short-circuit: second operand NOT evaluated if first is true
    side_effect_count = 0;
    int r3 = 1 || inc_and_return(1);
    if (side_effect_count != 0) return 5;
    if (r3 != 1) return 6;

    // || evaluates both when first is false
    side_effect_count = 0;
    int r4 = 0 || inc_and_return(1);
    if (side_effect_count != 1) return 7;
    if (r4 != 1) return 8;

    // Chained short-circuit
    side_effect_count = 0;
    int r5 = 1 && 1 && inc_and_return(0) && inc_and_return(99);
    if (side_effect_count != 1) return 9; // only first inc called, returns 0 → stops
    if (r5 != 0) return 10;

    // Complex expression with both
    int a = 5, b = 0, c = 10;
    int r6 = (a > 0 && b == 0) || (c < 0);
    if (r6 != 1) return 11;

    int r7 = (a < 0 && b == 0) || (c > 0);
    if (r7 != 1) return 12;

    int r8 = (a < 0 && b == 0) || (c < 0);
    if (r8 != 0) return 13;

    return 42;
}
