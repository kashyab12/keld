// Original second image: duplicate selectors require canonical SEL identity.
typedef unsigned long Word;
extern Class objc_getClass(const char *);
__attribute__((objc_root_class))
@interface SableOther { Class isa; }
+ (Word)classValue:(Word)value;
@end
@implementation SableOther
+ (Word)classValue:(Word)value {
    if ((Class)self != objc_getClass("SableOther") || _cmd != @selector(classValue:) || value != 37) return 3;
    return value + 6;
}
@end
Word sable_other_class(void) { return [SableOther classValue:37]; }

SEL sable_other_selector(void) { return @selector(classValue:); }
