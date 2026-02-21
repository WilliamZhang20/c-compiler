// EXPECT: 42
// Test: GNU statement expressions ({ ... })

int main() {
    // Basic statement expression
    int a = ({ 10 + 32; });
    if (a != 42) return 1;

    // Statement expression with declarations
    int b = ({
        int x = 20;
        int y = 22;
        x + y;
    });
    if (b != 42) return 2;

    // Statement expression in an expression context
    int c = 1 + ({
        int tmp = 40;
        tmp;
    });
    if (c != 41) return 3;

    return a; // 42
}
