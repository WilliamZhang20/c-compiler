// EXPECT: 10
// SLP: straight-line unrolled copy cluster (width 4 on SSE2 hosts)
int main() {
    int src[4];
    int dst[4];
    src[0] = 1;
    src[1] = 2;
    src[2] = 3;
    src[3] = 4;
    dst[0] = src[0];
    dst[1] = src[1];
    dst[2] = src[2];
    dst[3] = src[3];
    return dst[0] + dst[1] + dst[2] + dst[3];
}
