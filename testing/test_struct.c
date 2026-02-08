// EXPECT: 13
// Test basic struct support
struct Point {
    int x;
    int y;
};

int main() {
    struct Point p;
    p.x = 5;
    p.y = 8;
    return p.x + p.y;
}
