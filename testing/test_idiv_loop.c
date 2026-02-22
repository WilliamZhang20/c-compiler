// EXPECT: 42
int main() {
    int x = 84;
    int steps = 0;
    while (x > 1) {
        if (x % 2 == 0) {
            x = x / 2;
        } else {
            x = x - 1;
        }
        steps++;
    }
    // 84 -> 42 -> 21 -> 20 -> 10 -> 5 -> 4 -> 2 -> 1
    // steps = 8
    if (steps != 8) return steps;
    return 42;
}
