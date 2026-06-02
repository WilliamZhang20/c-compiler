// EXPECT: 0
struct Inner {
    int x;
    int y;
};

struct Outer {
    struct Inner inner;
};

int main(void) {
    struct Outer o = {.inner = {.x = 1, .y = 2}};
    if (o.inner.x != 1) return 1;
    if (o.inner.y != 2) return 2;
    return 0;
}
