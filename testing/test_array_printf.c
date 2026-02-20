// EXPECT: 0
int main() {
    int arr[5];
    int i;
    for (i = 0; i < 5; i = i + 1) {
        arr[i] = i * i;
    }
    for (i = 0; i < 5; i = i + 1) {
        printf("arr[%d] = %d\n", i, arr[i]);
    }
    return 0;
}
