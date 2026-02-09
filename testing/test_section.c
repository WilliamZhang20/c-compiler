// EXPECT: 123
// Test __attribute__((section("name"))) on globals
// The section attribute places variables in custom ELF sections

int __attribute__((section(".custom"))) custom_var = 123;

int main() {
    return custom_var;
}
