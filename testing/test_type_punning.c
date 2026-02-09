// EXPECT: 42
// Test complex pointer casts and type punning

union IntFloat {
    int i;
    float f;
};

int main() {
    union IntFloat u;
    u.i = 42;
    
    // Type punning through union
    int result = u.i;
    
    // Pointer casting
    int *p = &result;
    char *cp = (char*)p;
    
    // Cast back
    int *p2 = (int*)cp;
    
    return *p2;
}
