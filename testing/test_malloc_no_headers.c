// EXPECT: 0
extern int printf(const char* format, ...);
extern void* malloc(int size);
extern void free(void* ptr);

int main() {
    int* p = (int*)malloc(8);
    if (!p) return 1;
    *p = 123;
    printf("Malloc'd value: %d\n", *p);
    free(p);
    return 0;
}
