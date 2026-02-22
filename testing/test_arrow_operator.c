// EXPECT: 42
// Test struct with pointer member access via ->
struct Node {
    int value;
    int next_val;
};

struct Pair {
    int first;
    int second;
};

int get_value(struct Node *n) {
    return n->value;
}

int main() {
    struct Node n;
    n.value = 10;
    n.next_val = 20;

    struct Node *p = &n;

    // Arrow access
    int v1 = p->value;    // 10
    int v2 = p->next_val; // 20
    if (v1 != 10) return 1;
    if (v2 != 20) return 2;

    // Write via arrow
    p->value = 42;
    if (n.value != 42) return 3;

    // Pass struct pointer to function
    if (get_value(p) != 42) return 4;

    // Struct array with pointer iteration
    struct Pair pairs[3];
    pairs[0].first = 1; pairs[0].second = 2;
    pairs[1].first = 3; pairs[1].second = 4;
    pairs[2].first = 5; pairs[2].second = 6;

    int sum = 0;
    for (int i = 0; i < 3; i++) {
        struct Pair *pp = &pairs[i];
        sum = sum + pp->first + pp->second;
    }
    // 1+2+3+4+5+6 = 21
    if (sum != 21) return 5;

    return get_value(p); // 42
}
