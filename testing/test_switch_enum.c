// EXPECT: 2
enum Color { RED, GREEN, BLUE };

int main() {
    enum Color c = GREEN;
    int result = 0;
    switch (c) {
        case RED:
            result = 1;
            break;
        case GREEN:
            result = 2;
            break;
        case BLUE:
            result = 3;
            break;
        default:
            result = 99;
            break;
    }
    return result;
}
