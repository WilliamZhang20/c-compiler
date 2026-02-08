// EXPECT: 10
int main() {
    int sum = 0;
    for (int i = 0; i < 100; i = i + 1) {
        if (i >= 10) {
            break;
        }
        sum = sum + 1;
    }
    return sum;
}
