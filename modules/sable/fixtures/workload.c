// Original compiled-C workload: writable data, nested calls, flags and stack spills.
typedef unsigned long long u64;
static u64 data[64];
__attribute__((noinline)) static u64 mix(u64 *values, unsigned count) {
    u64 sum = 0;
    for (unsigned i = 0; i < count; i++) {
        u64 value = values[i];
        value = (value ^ (value >> 13)) * 17 + sum;
        values[i] = value;
        sum += value;
    }
    return sum;
}
u64 workload(void) {
    for (unsigned i = 0; i < 64; i++) data[i] = i + 1;
    u64 sum = 0;
    for (unsigned round = 0; round < 2000; round++) sum += mix(data, 64);
    return sum;
}
