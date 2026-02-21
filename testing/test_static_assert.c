// Test _Static_assert in the C compiler
// EXPECT: 42

_Static_assert(1, "true is true");
_Static_assert(sizeof(int) == 4, "int is 4 bytes");

int main(void) {
    _Static_assert(1, "inside function");
    _Static_assert(sizeof(char) == 1, "char is 1 byte");
    
    // C23 style without message
    _Static_assert(1);
    
    return 42;
}
