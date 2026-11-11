// Original headerless static protocol and scalar root-class execution probe.
typedef unsigned long Word;
typedef _Bool BOOL;
extern Class objc_getClass(const char *);
extern Protocol *objc_getProtocol(const char *);
extern const char *protocol_getName(Protocol *);
extern BOOL class_conformsToProtocol(Class, Protocol *);
@protocol SableScalar
- (Word)value:(Word)x;
@end
@protocol SableOtherScalar
- (Word)value:(Word)x;
@end
__attribute__((objc_root_class))
@interface ProtocolRoot <SableScalar> { Class isa; }
- (Word)value:(Word)x;
@end
@implementation ProtocolRoot
- (Word)value:(Word)x { return _cmd == @selector(value:) ? x + 5 : 1; }
@end
__attribute__((objc_root_class))
@interface UnrelatedRoot { Class isa; }
- (Word)value:(Word)x;
@end
@implementation UnrelatedRoot
- (Word)value:(Word)x { return x + 5; }
@end
__attribute__((objc_root_class))
@interface DeclaredOnlyRoot <SableScalar> { Class isa; }
@end
@implementation DeclaredOnlyRoot
@end
static struct { Class isa; } object;
static int same(const char *a, const char *b) {
    if (!a || !b) return 0;
    while (*a && *a == *b) { ++a; ++b; }
    return *a == *b;
}
extern long write(int,const void *,Word);
extern Word protocol_abi_call(Word,Word,Word,Word *);
static Word captured[36];
static volatile Word mutable_stamp=17;
static int ctor_result;
static int verify(void);
#if PROTOCOL_CTOR
__attribute__((constructor)) static void startup(void) {
    if (mutable_stamp!=17 || ctor_result) {ctor_result=1;return;}
    mutable_stamp=23;
    ctor_result=verify()==0 ? 7 : 1;
}
#endif
static int abi(Word fn,Word a,Word b,Word expected,BOOL boolean) {
    Word result=protocol_abi_call(fn,a,b,captured);
    if ((boolean ? result&1 : result)!=expected) return 1;
    for (Word i=21;i<=28;i++) if (captured[i]!=200+i) return 2;
    if (captured[19]!=(Word)captured || captured[20]!=fn) return 3;
    if (captured[18]!=captured[32] || captured[29]!=captured[33] || captured[31]!=captured[34] || captured[30]!=captured[35]) return 4;
    return 0;
}
static int verify(void) {
    // This ordinary __data storage must stay writable, beside protocol bodies.
    mutable_stamp=41;
    Class cls = objc_getClass("ProtocolRoot");
    Protocol *literal = @protocol(SableScalar);
    Protocol *lookup = objc_getProtocol("SableScalar");
    if (!cls || !literal || literal != lookup) return 10;
    if (!same(protocol_getName(lookup), "SableScalar")) return 11;
    if (!class_conformsToProtocol(cls, literal)) return 12;
    if (class_conformsToProtocol(objc_getClass("UnrelatedRoot"), literal)) return 13;
    if (objc_getProtocol("SableMissing")) return 14;
    Protocol *other=@protocol(SableOtherScalar);
    if (!other || other==literal || other!=objc_getProtocol("SableOtherScalar")) return 24;
    if (!same(protocol_getName(other),"SableOtherScalar") || class_conformsToProtocol(cls,other)) return 25;
    if (!same(protocol_getName((Protocol *)0), "nil")) return 15;
    if (class_conformsToProtocol((Class)0, literal)) return 16;
    if (class_conformsToProtocol(cls, (Protocol *)0)) return 17;
    if (!class_conformsToProtocol(objc_getClass("DeclaredOnlyRoot"),literal)) return 19;
    if (abi((Word)objc_getProtocol,(Word)"SableScalar",0,(Word)lookup,0)) return 20;
    if (abi((Word)protocol_getName,(Word)lookup,0,(Word)protocol_getName(lookup),0)) return 21;
    if (abi((Word)class_conformsToProtocol,(Word)cls,(Word)literal,1,1)) return 22;
    if (mutable_stamp!=41) return 23;
    object.isa = cls;
    return [(ProtocolRoot *)&object value:37] == 42 ? 0 : 18;
}

int sable_main(void) {
#if PROTOCOL_CTOR
    if (ctor_result!=7 || mutable_stamp!=41) return 30;
#else
    if (ctor_result || mutable_stamp!=17) return 31;
#endif
    int result=verify();
    if (result) return result;
    static const char message[]="Protocol lookup, names, declared adoption and guest IMP OK\n";
    write(1,message,sizeof(message)-1);
    return 0;
}
