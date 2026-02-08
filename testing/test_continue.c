// EXPECT: 40
// Test continue statement - skip when i equals 5
int main() {
    int sum = 0;
    for (int i = 0; i < 10; i = i + 1) {
        if (i == 5) {
            continue;
        }
        sum = sum + i;
    }
    return sum;
}
