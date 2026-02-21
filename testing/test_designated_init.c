// EXPECT: 0
// Test struct designated initializers with out-of-order fields

struct Rect {
    int x;
    int y;
    int width;
    int height;
};

int main() {
    // Out-of-order designated initializer
    struct Rect r = {.width = 100, .height = 50, .x = 10, .y = 20};
    if (r.x != 10) return 1;
    if (r.y != 20) return 2;
    if (r.width != 100) return 3;
    if (r.height != 50) return 4;

    // Partial initialization (only some fields)
    struct Rect r2 = {.x = 5, .height = 30};
    if (r2.x != 5) return 5;
    if (r2.height != 30) return 6;

    return 0;
}
