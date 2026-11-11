// Original headerless declarations. The ARC interior-pointer contract is retained.
typedef unsigned long Word;
__attribute__((objc_root_class))
@interface NSString { @public Class isa; Word flags; Word bytesBits; Word count; }
- (const char *)UTF8String __attribute__((objc_returns_inner_pointer));
- (NSString *)uppercaseString;
- (void)dealloc;
@end
@interface SliceLife : NSString
+ (id)make:(Word)tag;
@end
extern Word string_destroyed(void), life_count(void), life_order(Word);
