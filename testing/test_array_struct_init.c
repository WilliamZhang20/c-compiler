// EXPECT: 0
// Test array of structs with initializer lists

struct Vec2 {
    int x;
    int y;
};

int main() {
    // Array of structs
    struct Vec2 points[3] = {{1, 2}, {3, 4}, {5, 6}};
    if (points[0].x != 1) return 1;
    if (points[0].y != 2) return 2;
    if (points[1].x != 3) return 3;
    if (points[1].y != 4) return 4;
    if (points[2].x != 5) return 5;
    if (points[2].y != 6) return 6;

    // Sum all components
    int sum = points[0].x + points[0].y + points[1].x + points[1].y + points[2].x + points[2].y;
    if (sum != 21) return 7;

    return 0;
}
