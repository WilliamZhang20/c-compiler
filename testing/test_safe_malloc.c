// EXPECT: 42
int main() {
    int* p;
    p = (int*)malloc(sizeof(int));
    *p = 42;
    int result = *p;
    free(p);
    return result;
}
