typedef unsigned long long u64;
extern u64 sable_mix(u64);
static u64 (*volatile mixer)(u64) = sable_mix;
#ifndef SABLE_ROUNDS
#define SABLE_ROUNDS 2000
#endif
u64 workload(void) {
    u64 value = 1;
    for (unsigned i = 0; i < SABLE_ROUNDS; i++) value = mixer(value);
    return value;
}
