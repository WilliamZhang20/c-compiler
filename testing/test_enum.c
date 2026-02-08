// Test: Basic enum support
// EXPECT: 12

enum Color {
    RED,
    GREEN,
    BLUE
};

enum Status {
    OK = 0,
    ERROR = -1,
    PENDING = 10
};

int main() {
    int x = BLUE;     // BLUE = 2
    int y = PENDING;  // PENDING = 10
    return x + y;     // 2 + 10 = 12
}
