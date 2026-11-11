// Original, headerless Objective-C2 scalar metadata/dispatch probe.
// No Foundation, ARC, allocation, autorelease, exception or Apple implementation.
typedef unsigned long Word;
extern Class objc_getClass(const char *name);
extern Word sable_abi_imp(id,SEL,Word,Word,Word,Word,Word,Word);
static Class expected_class;
static id expected_instance;
__attribute__((objc_root_class))
@interface SableRoot {
    Class isa;
}
+ (Word)classValue:(Word)value;
- (Word)add:(Word)a to:(Word)b;
- (Word)nested:(Word)value;
- (Word)abi:(Word)a b:(Word)b c:(Word)c d:(Word)d e:(Word)e f:(Word)f;
#if OBJC_UNKNOWN
- (Word)missing:(Word)value;
#endif
@end
@implementation SableRoot
+ (Word)classValue:(Word)value {
    if ((Class)self != expected_class || _cmd != @selector(classValue:) || value != 37) return 1;
    return value + 5;
}
- (Word)add:(Word)a to:(Word)b {
    if (self != expected_instance || _cmd != @selector(add:to:) || a != 17 || b != 25) return 2;
    return a + b;
}
- (Word)nested:(Word)value { return [self add:17 to:25] + value; }
- (Word)abi:(Word)a b:(Word)b c:(Word)c d:(Word)d e:(Word)e f:(Word)f {
    return sable_abi_imp(self,_cmd,a,b,c,d,e,f);
}
@end
extern Word sable_other_class(void);
extern SEL sable_other_selector(void);
extern Word sable_abi_send(id, SEL);
extern Word abi_capture[36];
static struct { Class isa; } instance;
extern long write(int, const void *, unsigned long);
static int ctor_result;
static void setup(void) {
    expected_class = objc_getClass("SableRoot");
    instance.isa = expected_class;
    expected_instance = (id)&instance;
}
#if OBJC_CTOR
__attribute__((constructor)) static void initialize_probe(void) {
    if (instance.isa || expected_class || expected_instance || ctor_result) {ctor_result=1;return;}
    setup();
    ctor_result = [(SableRoot *)&instance add:17 to:25] == 42 ? 7 : 1;
}
#endif
int sable_main(void) {
#if OBJC_CTOR
    if (ctor_result != 7 || instance.isa != expected_class) return 14;
#else
    if (instance.isa || expected_class || expected_instance) return 15;
#endif
    Class cls = objc_getClass("SableRoot");
    if (!cls) return 10;
    instance.isa = cls;
    expected_class = cls;
    expected_instance = (id)&instance;
    SableRoot *object = (SableRoot *)&instance;
    if ([object add:17 to:25] != 42) return 11;
    if ([SableRoot classValue:37] != 42) return 12;
    if ([(SableRoot *)0 add:17 to:25] != 0) return 13;
    if (objc_getClass("UnknownSableClass")) return 16;
    if (sable_other_selector() != @selector(classValue:)) return 23;
    if ([object nested:5] != 47 || sable_other_class() != 43) return 17;
    if (sable_abi_send((id)object,@selector(abi:b:c:d:e:f:)) != 42) return 18;
    SEL abi_selector=@selector(abi:b:c:d:e:f:);
    if (abi_capture[0] != (Word)object || abi_capture[1] != (Word)abi_selector) return 19;
    for (Word i=2;i<=8;i++) if (abi_capture[i] != 100+i) return 20;
    if (abi_capture[18] != abi_capture[32] || abi_capture[29] != abi_capture[33]) return 21;
    if (abi_capture[31] != abi_capture[34] || abi_capture[30] != abi_capture[35]) return 22;
#if OBJC_UNKNOWN
    return (int)[object missing:1];
#endif
    static const char message[] = "ObjC root class, scalar instance/class dispatch, nil and selectors OK\n";
    write(1, message, sizeof(message)-1);
    return 0;
}
