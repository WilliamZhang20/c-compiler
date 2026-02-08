// Struct manipulation - tests struct field access performance  
struct Point {
    int x;
    int y;
    int z;
};

int main() {
    struct Point p1;
    struct Point p2;
    int i;
    int total = 0;
    int dx;
    int dy;
    int dz;
    
    p1.x = 10;
    p1.y = 20;
    p1.z = 30;
    
    for (i = 0; i < 100; i = i + 1) {
        p2.x = i;
        p2.y = i * 2;
        p2.z = i * 3;
        
        // Calculate distance squared inline
        dx = p1.x - p2.x;
        dy = p1.y - p2.y;
        dz = p1.z - p2.z;
        total = total + (dx * dx + dy * dy + dz * dz);
    }
    
    return total % 256;
}
