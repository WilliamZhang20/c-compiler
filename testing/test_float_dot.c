// EXPECT: 0
int main() {
    float f = .5;
    if (f != 0.5) return 1;
    
    float g = .123;
    if (g > 0.124) return 2;
    if (g < 0.122) return 3;
    
    return 0;
}
