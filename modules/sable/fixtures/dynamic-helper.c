// Original shared-library dependency. No Apple libraries or SDK headers.
typedef unsigned long long u64;
u64 sable_bias(u64 value) { return (value ^ (value >> 7)) + 19; }
