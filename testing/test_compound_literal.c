// EXPECT: 42
// Test: compound literals (type){init}

struct point {
    int x;
    int y;
};

int main() {
    // Scalar compound literal
    int a = (int){42};
    if (a != 42) return 1;

    // Struct compound literal  
    struct point p = (struct point){10, 32};
    if (p.x != 10) return 2;
    if (p.y != 32) return 3;

    return a; // 42
}
