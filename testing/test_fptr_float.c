// EXPECT: 0
float add(float a, float b) {
    return a + b;
}

int main() {
    float (*fp)(float, float) = add;
    float res = fp(1.5, 2.5);
    if (res != 4.0) return 1;
    return 0;
}
