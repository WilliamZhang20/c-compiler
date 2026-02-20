#include <stdio.h>

int main() {
    int j = 1;
    for(int i=0; i<5; i++) {
        printf("Hello from C compiler in Rust with an i value of %d\n", j);
        j += 1;
    }
    return 0;
}