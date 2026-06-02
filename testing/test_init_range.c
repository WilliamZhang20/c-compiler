// EXPECT: 4
int main(void) {
    int arr[10] = {[0 ... 4] = 1, [5] = 2};
    return arr[0] + arr[4] + arr[5];
}
