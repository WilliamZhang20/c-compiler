// EXPECT: 84
// Test function pointers in structs (pointer-to-member semantics)

struct Operations {
    int (*add)(int, int);
    int (*mul)(int, int);
};

int my_add(int a, int b) {
    return a + b;
}

int my_mul(int a, int b) {
    return a * b;
}

int main() {
    struct Operations ops;
    ops.add = my_add;
    ops.mul = my_mul;
    
    int x = ops.add(10, 20);   // 30
    int y = ops.mul(3, 18);     // 54
    
    return x + y;  // 84
}
