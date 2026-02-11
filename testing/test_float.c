// EXPECT: 0

int test_float_basic();
int test_float_dot();
int test_float_init();
int test_float_int();
int test_float_simple();
int test_float_var();

int main() {
    if (test_float_basic() != 0) return 1;
    if (test_float_dot() != 0) return 2;
    if (test_float_init() != 10) return 3;
    if (test_float_int() != 0) return 4;
    if (test_float_simple() != 0) return 5;
    if (test_float_var() != 0) return 6;
    return 0;
}

int test_float_basic() {
    float a = 1.5;
    float b = 2.5;
    float c = a + b;
    if (c != 4.0) {
        return 1;
    }
    return 0;
}

int test_float_dot() {
    float x = .5;
    if (x != 0.5) {
        return 1;
    }
    return 0;
}

int test_float_init() {
    float a;
    a = 10.0;
    return (int)a;
}

int test_float_int() {
    float a = 5.5;
    int b = (int)a;
    if (b != 5) {
        return 1;
    }
    float c = (float)b;
    if (c != 5.0) {
        return 2;
    }
    return 0;
}

int test_float_simple() {
    float a = 1.0;
    if (a != 1.0) {
        return 1;
    }
    return 0;
}

int test_float_var() {
    float a = 1.5;
    float b = a;
    if (b != 1.5) {
        return 1;
    }
    return 0;
}
