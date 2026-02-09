// EXPECT: 123
// Test volatile global variable
volatile int sensor_value = 123;

int main() {
    return sensor_value;
}
