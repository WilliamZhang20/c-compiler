#include <stdio.h>
#include <stdlib.h>

int main() {
    int* p = (int*)malloc(8); // Using 8 to match our compiler's int size
    if (!p) return 1;
    *p = 123;
    printf("Malloc'd value: %d\n", *p);
    free(p);
    return 0;
}
