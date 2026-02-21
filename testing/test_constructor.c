// Test __attribute__((constructor)) and __attribute__((destructor))
// Constructor sets a global variable before main runs
// EXPECT: 42

int init_val = 0;

__attribute__((constructor))
void setup(void) {
    init_val = 42;
}

int main(void) {
    return init_val;  // Should be 42, set by constructor before main
}
