// Test function pointer typedefs
// EXPECT: 0
typedef int (*simple_funcptr)(void);
typedef void (*another_funcptr)(int x);

int main() {
    return 0;
}
