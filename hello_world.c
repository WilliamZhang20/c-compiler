#include <stdio.h>

int main() {
    int j = 1;
    for(int i=0; i<5; i++) {
        j += 1;
    }
    printf("Hello from C compiler in Rust with an i value of %d\n", j);
    return 0;
}