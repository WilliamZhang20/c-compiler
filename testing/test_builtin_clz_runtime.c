// EXPECT: 26
// Runtime __builtin_clz on non-constant value
int clz_var(unsigned x) {
    return __builtin_clz(x);
}

int main(void) {
    unsigned v = 32;
    return clz_var(v);
}
