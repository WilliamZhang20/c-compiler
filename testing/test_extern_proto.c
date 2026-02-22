// EXPECT: 42
// Test extern declarations and forward struct declarations
struct forward_decl;  // forward declaration

extern int external_val;  // extern declaration (won't be linked, just parsed)

// Function prototype
int compute(int x, int y);

int compute(int x, int y) {
    return x + y;
}

int main(void) {
    return compute(20, 22);
}
