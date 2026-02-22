// EXPECT: 42
// Test nested structs: struct containing another struct, deep member access
struct Inner {
    int a;
    int b;
};

struct Outer {
    struct Inner inner;
    int c;
};

struct Deep {
    struct Outer outer;
    int d;
};

int sum_inner(struct Inner s) {
    return s.a + s.b;
}

int main() {
    struct Outer o;
    o.inner.a = 10;
    o.inner.b = 5;
    o.c = 7;

    // Access nested struct fields
    int x = o.inner.a + o.inner.b + o.c; // 22

    // Pass nested struct to function
    int y = sum_inner(o.inner); // 15

    // Three-level nesting
    struct Deep d;
    d.outer.inner.a = 1;
    d.outer.inner.b = 2;
    d.outer.c = 3;
    d.d = 4;
    int z = d.outer.inner.a + d.outer.inner.b + d.outer.c + d.d; // 10

    // Verify: 22 + 15 + 10 = 47... adjust to get 42:
    // Let's just use simple values
    if (x != 22) return 1;
    if (y != 15) return 2;
    if (z != 10) return 3;

    // 22 + 15 + 10 - 5 = 42
    return x + y + z - 5;
}
