// Test: Basic float support (parsing/lexing only)
// EXPECT: 0
// Note: Float arithmetic not yet implemented, just testing that float types compile

float add_floats(float a, float b) {
    return a + b;
}

double add_doubles(double x, double y) {
    return x + y;
}

int main() {
    float f = 3.14;
    double d = 2.718;
    float sum = add_floats(f, 1.0);
    double dsum = add_doubles(d, 1.0);
    
    // Return 0 since float ops aren't implemented yet
    return 0;
}
