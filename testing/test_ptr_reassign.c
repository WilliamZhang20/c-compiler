// Debug pointers step by step
// EXPECT: 8
int main() {
    int x = 5;
    int y = 8;
    int *p = &x;
    p = &y;  // Reassign p to point to y
    return *p;
}
