#include "text.h"
extern id class_createInstance(Class,Word);
extern id object_dispose(id);
extern id objc_autorelease(id);
extern char string_class __asm__("_OBJC_CLASS_$_NSString");
static Word destroyed, deaths, order[16];
Word string_destroyed(void) { return destroyed; }
Word life_count(void) { return deaths; }
Word life_order(Word n) { return n<deaths ? order[n] : 0; }
@implementation NSString
- (const char *)UTF8String { return (const char *)bytesBits; }
- (NSString *)uppercaseString {
    if(count>63)return (id)0;
    const unsigned char *in=(const unsigned char *)bytesBits;
    NSString *result=class_createInstance((Class)&string_class,count+1);
    if(!result)return (id)0;
    unsigned char *out=(unsigned char *)result+32;
    for(Word n=0;n<count;n++) {
        unsigned c=in[n];
        out[n]=(c>='a'&&c<='z') ? c-('a'-'A') : c;
    }
    out[count]=0; result->bytesBits=(Word)out;result->count=count;
    return objc_autorelease(result);
}
- (void)dealloc { destroyed++; object_dispose(self); }
@end
@implementation SliceLife
+ (id)make:(Word)tag {
    SliceLife *result=class_createInstance(self,0);
    if(result)result->flags=tag;
    return result;
}
- (void)dealloc {
    if(deaths<16)order[deaths++]=flags;
    object_dispose(self);
}
@end
