#include <stdio.h>

int main() {
    int a = 10;
    int b = 20;
    int c = 30;
    int d = 40;
    
    // Test postfix increment
    int x = a++;  // x = 10, a = 11
    printf("After a++: x=%d, a=%d\n", x, a);
    
    // Test postfix decrement
    int y = b--;  // y = 20, b = 19
    printf("After b--: y=%d, b=%d\n", y, b);
    
    // Test prefix increment
    int z = ++c;  // z = 31, c = 31
    printf("After ++c: z=%d, c=%d\n", z, c);
    
    // Test prefix decrement
    int w = --d;  // w = 39, d = 39
    printf("After --d: w=%d, d=%d\n", w, d);
    
    // Test in for loop
    int sum = 0;
    for (int i = 0; i < 5; i++) {
        sum += i;
    }
    printf("Sum 0 to 4: %d\n", sum);
    
    return 0;
}
