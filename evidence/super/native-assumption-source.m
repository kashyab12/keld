// Original headerless scalar inheritance and Super2 execution contract.
typedef unsigned long Word;
extern Class objc_getClass(const char *);
extern Word sable_abi_imp(id,SEL,Word,Word,Word,Word,Word,Word);
#if SUPER_PROTOCOL
@protocol SableAdopted
- (Word)inherited:(Word)x;
@end
extern _Bool class_conformsToProtocol(Class,Protocol *);
#endif
__attribute__((objc_root_class))
@interface SableBase
#if SUPER_PROTOCOL
<SableAdopted>
#endif
{ Class isa; }
+ (Word)value:(Word)x;
- (Word)value:(Word)x;
+ (Word)inherited:(Word)x;
- (Word)inherited:(Word)x;
- (Word)abi:(Word)a b:(Word)b c:(Word)c d:(Word)d e:(Word)e f:(Word)f;
@end
#if !SUPER_SPLIT || SUPER_BASE_ONLY
@implementation SableBase
+ (Word)value:(Word)x { return _cmd == @selector(value:) ? x + 3 : 1; }
- (Word)value:(Word)x { return _cmd == @selector(value:) ? x + 5 : 2; }
+ (Word)inherited:(Word)x { return _cmd == @selector(inherited:) ? x + 13 : 3; }
- (Word)inherited:(Word)x { return _cmd == @selector(inherited:) ? x + 9 : 4; }
- (Word)abi:(Word)a b:(Word)b c:(Word)c d:(Word)d e:(Word)e f:(Word)f {
    return sable_abi_imp(self,_cmd,a,b,c,d,e,f);
}
@end
#endif
#if !SUPER_BASE_ONLY
@interface SableChild : SableBase
+ (Word)value:(Word)x;
- (Word)value:(Word)x;
- (Word)nested:(Word)x;
@end
@implementation SableChild
+ (Word)value:(Word)x { return [super value:x] + 11; }
- (Word)value:(Word)x { return [super value:x] + 7; }
- (Word)nested:(Word)x { return [super value:x] + [self value:x]; }
@end
#if SUPER_DEEP
@interface SableGrandchild : SableChild
@end
@implementation SableGrandchild
@end
#endif
extern Word sable_abi_super(id,Class,SEL);
extern Word abi_capture[36];
extern long write(int,const void *,Word);
static struct { Class isa; } instance;
static int ctor_result;
static volatile Word mutable_stamp=17;
static int verify(void) {
    Class cls=objc_getClass("SableChild");
    if (!cls) return 10;
#if SUPER_PROTOCOL
    // This valid native query requires inherited class conformance. Sable's
    // direct-only service must reject it, rather than silently returning false.
    if (!class_conformsToProtocol(cls,@protocol(SableAdopted))) return 21;
#endif
    instance.isa=cls;
    SableChild *object=(SableChild *)&instance;
    if ([object value:37]!=49) return 11;
    if ([SableChild value:37]!=51) return 12;
    if ([object inherited:37]!=46 || [SableChild inherited:37]!=50) return 13;
    if ([object nested:37]!=91) return 14;
    if ([(SableChild *)0 value:37]!=0) return 15;
    SEL selector=@selector(abi:b:c:d:e:f:);
    if (sable_abi_super((id)object,cls,selector)!=42) return 16;
    if (abi_capture[0]!=(Word)object || abi_capture[1]!=(Word)selector) return 17;
    for (Word i=2;i<=8;i++) if (abi_capture[i]!=100+i) return 18;
    if (abi_capture[18]!=abi_capture[32] || abi_capture[29]!=abi_capture[33]) return 19;
    if (abi_capture[31]!=abi_capture[34] || abi_capture[30]!=abi_capture[35]) return 20;
    mutable_stamp=41;
    return 0;
}
#if SUPER_CTOR
__attribute__((constructor)) static void startup(void) {
    if (instance.isa || mutable_stamp!=17 || ctor_result) {ctor_result=1;return;}
    ctor_result=verify()==0 ? 7 : 1;
}
#endif
int sable_main(void) {
#if SUPER_CTOR
    if (ctor_result!=7 || mutable_stamp!=41) return 30;
#else
    if (ctor_result || instance.isa || mutable_stamp!=17) return 31;
#endif
    int result=verify();
    if (result) return result;
    static const char message[]="Superclass lookup, Super2 guest methods, nesting and ABI OK\n";
    write(1,message,sizeof(message)-1);
    return 0;
}
#endif
