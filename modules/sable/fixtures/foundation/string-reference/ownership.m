// Original MRC observer. Apple's installed runtime supplies native ownership.
#import <Foundation/Foundation.h>
#include <stdint.h>
#include <stdio.h>
extern void *objc_autoreleasePoolPush(void);
extern void objc_autoreleasePoolPop(void *);
extern id objc_retain(id);
extern void objc_release(id);
extern id objc_retainAutorelease(id);
extern id objc_autoreleaseReturnValue(id);
extern id probe_claim_fallback(id);
extern id probe_claim_candidate(unsigned long);
extern void probe_release_x20(id);
extern uint64_t release_before[32], release_after[32];
static volatile unsigned long dead_mask;
static unsigned long destruction_order[16], destruction_count;
@interface SliceLife : NSObject { unsigned long tag; }
- (id)initTag:(unsigned long)value;
@end
@implementation SliceLife
- (id)initTag:(unsigned long)value { self = [super init]; if (self) tag=value; return self; }
- (void)dealloc {
    if (tag < 16) dead_mask |= 1UL<<tag;
    if (destruction_count<16) destruction_order[destruction_count++]=tag;
    [super dealloc];
}
@end
__attribute__((noinline)) id probe_owned(unsigned long tag) {
    return [[SliceLife alloc] initTag:tag];
}
__attribute__((noinline)) id probe_candidate_producer(unsigned long tag) {
    return objc_autoreleaseReturnValue(probe_owned(tag));
}
int main(void) {
    int error=0;
    void *outer=objc_autoreleasePoolPush();
    id outside=probe_owned(1);
    if (objc_retainAutorelease(outside)!=outside) error=10;
    objc_release(outside); // Outer pool now owns the remaining deferred release.
    void *inner=objc_autoreleasePoolPush();
    id a=probe_owned(2);
    if (objc_retainAutorelease(a)!=a) error=11;
    objc_release(a);
    id held=objc_retain(a);
    id inner_only=probe_owned(3);
    objc_retainAutorelease(inner_only);objc_release(inner_only);
    objc_autoreleasePoolPop(inner);
    unsigned long after_inner=dead_mask;
    if ((dead_mask & (1UL<<3))==0 || (dead_mask & ((1UL<<1)|(1UL<<2)))) error=12;
    objc_release(held);
    if ((dead_mask & (1UL<<2))==0) error=13;
    // No +0-return producer participates here: this must be ordinary retain fallback.
    inner=objc_autoreleasePoolPush();
    id f=probe_owned(4);
    id claimed=probe_claim_fallback(f);
    if (claimed!=f) error=14;
    objc_release(f);
    objc_autoreleasePoolPop(inner);
    int fallback_survived=(dead_mask & (1UL<<4))==0;
    if (!fallback_survived) error=15;
    objc_release(claimed);
    if ((dead_mask & (1UL<<4))==0) error=16;
    // Adjacent autoreleaseReturnValue producer + claim. Timing distinguishes handoff/fallback.
    inner=objc_autoreleasePoolPush();
    id c=probe_claim_candidate(5);
    objc_release(c);
    int candidate_dead_before_pop=(dead_mask & (1UL<<5))!=0;
    objc_autoreleasePoolPop(inner);
    if ((dead_mask & (1UL<<5))==0) error=17;
    // Custom helper argument lives in x20; x0 is an independent sentinel.
    id r=probe_owned(6);
    probe_release_x20(r);
    if ((dead_mask & (1UL<<6))==0) error=18;
    uint64_t preserved=0;
    for(unsigned i=0;i<32;i++) if(release_before[i]==release_after[i]) preserved|=1ULL<<i;
    // Ordinary call ABI: caller-clobbered x0-x17 are observations, not preservation promises.
    // x18 is compared at its true platform baseline; x30 resume is observed separately.
    uint64_t required=0xbffc0000ULL; // x18-x29 and SP.
    if((preserved & required)!=required) error=19;
    objc_autoreleasePoolPop(outer);
    if(dead_mask != 0x7e || destruction_count != 6) error=20;
    printf("{\"kind\":\"native ownership observer\",\"error\":%d,\"after_inner_mask\":%lu,\"final_dead_mask\":%lu,\"fallback_survived_pool\":%d,\"candidate_dead_before_pop\":%d,\"release_x20_preserved_mask\":\"%08llx\",\"release_x20_required_mask\":\"%08llx\",\"order\":[",error,after_inner,dead_mask,fallback_survived,candidate_dead_before_pop,(unsigned long long)preserved,(unsigned long long)required);
    for(unsigned i=0;i<destruction_count;i++)printf("%s%lu",i?",":"",destruction_order[i]);
    puts("]}");
    return error;
}
