typedef unsigned long long u64;
extern u64 sable_bias(u64);
static u64 calls;
static u64 *volatile counter = &calls; // exercises slid-image pointer rebasing
static u64 (*volatile bias)(u64) = sable_bias; // transitive ordinal binding
u64 sable_mix(u64 value) {
    u64 *p = counter;
    (*p)++;
    return bias((value ^ (value >> 13)) * 17 + *p);
}
