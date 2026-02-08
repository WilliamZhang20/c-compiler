// Test structs
struct Point {
    int x;
    int y;
};

int test_struct() {
    struct Point p;
    p.x = 10;
    p.y = 20;
    return p.x + p.y; // Should return 30
}

// Test typedefs
typedef int MyInt;
int test_typedef() {
    MyInt a = 42;
    return a; // Should return 42
}

// Test switch
int test_switch(int x) {
    switch (x) {
        case 1: return 10;
        case 2: return 20;
        default: return 30;
    }
}

int main() {
    if (test_struct() != 30) return 1;
    if (test_typedef() != 42) return 2;
    if (test_switch(1) != 10) return 3;
    if (test_switch(2) != 20) return 4;
    if (test_switch(3) != 30) return 5;
    return 0; // Success
}
