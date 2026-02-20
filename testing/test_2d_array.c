// EXPECT: 4
int main() {
    int arr[2][2];
    arr[0][0] = 1;
    arr[0][1] = 2;
    arr[1][0] = 3;
    arr[1][1] = 4;
    return arr[1][1];
}
