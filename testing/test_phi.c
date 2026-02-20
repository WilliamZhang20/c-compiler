// EXPECT: 100
int main() {
    int x = 10;
    if (x > 5) {
        x = 100;
    } else {
        x = 0;
    }
    return x;
}
