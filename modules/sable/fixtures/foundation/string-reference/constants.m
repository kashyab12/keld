// Original ARC native reference: constant records, cross-image identity and interior-pointer lifetime.
#import <Foundation/Foundation.h>
#import <objc/runtime.h>
#include <stdint.h>
#include <stdio.h>
#include <string.h>
extern int __CFConstantStringClassReference[];
extern id otherLiteral(void);
struct ConstantRecord { uintptr_t isa; uint32_t flags; uint32_t padding; const char *bytes; uint64_t length; };
int main(void) {
    int error=0, cross_same=0, isa_symbol=0, class_symbol=0, utf8_alias=0, weak_alive=0;
    uint32_t flags=0; uint64_t length=0;
    __weak NSString *weak_literal;
    __weak NSString *weak_result;
    @autoreleasepool {
        NSString *literal=@"sable foundation";
        NSString *other=otherLiteral();
        struct ConstantRecord r;memcpy(&r,(__bridge const void *)literal,sizeof(r));
        flags=r.flags;length=r.length;
        isa_symbol=r.isa==(uintptr_t)__CFConstantStringClassReference;
        class_symbol=object_getClass(literal)==(__bridge Class)(void *)__CFConstantStringClassReference;
        cross_same=literal==other;
        if(r.flags!=0x7c8 || r.length!=16 || memcmp(r.bytes,"sable foundation",17))error=10;
        if(!isa_symbol || !class_symbol || strcmp([other UTF8String],"sable foundation"))error=11;
        weak_literal=literal;
        NSString *value=[literal uppercaseString];
        if(value==literal || strcmp([value UTF8String],"SABLE FOUNDATION"))error=12;
        weak_result=value;
        const char *bytes=[value UTF8String];
        utf8_alias=bytes==r.bytes;
        value=nil; // Returned bytes must remain usable within this pool.
        weak_alive=weak_result!=nil;
        if(!weak_alive || strcmp(bytes,"SABLE FOUNDATION") || strlen(bytes)!=16)error=13;
    }
    int result_dead=weak_result==nil;
    int literal_alive=weak_literal!=nil;
    if(!result_dead || !literal_alive)error=14;
    printf("{\"kind\":\"native string/constant observer\",\"error\":%d,\"cfstring_size\":%zu,\"flags\":%u,\"length\":%llu,\"isa_is_class_reference_address\":%d,\"class_is_class_reference_address\":%d,\"same_literal_across_images\":%d,\"uppercase_utf8_is_literal_bytes\":%d,\"result_alive_after_strong_nil_inside_pool\":%d,\"result_dead_after_pool\":%d,\"literal_alive_after_pool\":%d}\n",error,sizeof(struct ConstantRecord),flags,(unsigned long long)length,isa_symbol,class_symbol,cross_same,utf8_alias,weak_alive,result_dead,literal_alive);
    return error;
}
