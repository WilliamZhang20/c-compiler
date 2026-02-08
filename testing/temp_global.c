// EXPECT: 30
int g_x = 10;
int g_y;

int main() {
    g_y = 20;
    return g_x + g_y;
}
