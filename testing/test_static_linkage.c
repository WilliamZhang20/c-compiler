// EXPECT: 15
// Test static functions and variables
static int counter = 10;

static int add_five(int x) {
    return x + 5;
}

int main(void) {
    return add_five(counter);
}
