// Test function pointers with printf debugging
#include <stdio.h>

int add(int a, int b) {
    int result = a + b;
    printf("add(%d, %d) = %d\n", a, b, result);
    return result;
}

int multiply(int a, int b) {
    int result = a * b;
    printf("multiply(%d, %d) = %d\n", a, b, result);
    return result;
}

int main() {
    int (*op)(int, int);
    
    op = add;
    int result1 = op(10, 5);
    printf("result1 = %d\n", result1);
    
    op = multiply;
    int result2 = op(3, 9);
    printf("result2 = %d\n", result2);
    
    int final = result1 + result2;
    printf("final = %d + %d = %d\n", result1, result2, final);
    return final;
}
