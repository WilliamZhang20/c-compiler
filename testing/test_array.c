// EXPECT: 30
int main() {
    int a[3];
    a[0] = 10;
    a[1] = 20;
    a[2] = a[0] + a[1];
    return a[2];
}
