// EXPECT: 1
int main() {
    int x = 10 + 20;
    int y = x * 2;
    if (y > 50) {
        return 1;
    } else {
        return 0;
    }
}
