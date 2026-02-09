// EXPECT: 7
// Test bit fields in structs

struct Flags {
    unsigned int flag1 : 1;
    unsigned int flag2 : 3;
    unsigned int flag3 : 4;
};

int main() {
    struct Flags f;
    f.flag1 = 1;
    f.flag2 = 2;
    f.flag3 = 4;
    return f.flag1 + f.flag2 + f.flag3;
}
