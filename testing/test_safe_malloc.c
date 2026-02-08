int main() {
    int* p;
    p = (int*)malloc(sizeof(int));
    *p = 42;
    free(p);
    return *p; // Should be 222 (0xDE poisoned)
}
