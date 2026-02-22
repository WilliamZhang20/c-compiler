// EXPECT: 55
// Test do-while loops: basic, nested, with break/continue
int main() {
    // Basic do-while: sum 1..10
    int sum = 0;
    int i = 1;
    do {
        sum = sum + i;
        i++;
    } while (i <= 10);
    // sum = 55

    // do-while always executes body at least once
    int ran = 0;
    do {
        ran = 1;
    } while (0);
    if (!ran) return 1;

    // do-while with break
    int count = 0;
    do {
        count++;
        if (count == 5) break;
    } while (count < 100);
    if (count != 5) return 2;

    // do-while with continue
    int evens = 0;
    int j = 0;
    do {
        j++;
        if (j % 2 != 0) continue;
        evens++;
    } while (j < 10);
    if (evens != 5) return 3;

    return sum; // 55
}
