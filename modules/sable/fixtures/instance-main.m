// Original headerless fixed-layout instance creation, pointer ABI and disposal.
typedef unsigned long Word;
extern Class objc_getClass(const char *);
extern id class_createInstance(Class,Word);
extern id object_dispose(id);
__attribute__((objc_root_class))
@interface AllocBase { Class isa; }
+ (id)make:(Word)extra;
- (id)identity;
- (id)choose:(id)other;
- (Word)matches:(id)other;
- (Word)value:(Word)x;
@end
#if !INSTANCE_SPLIT || INSTANCE_BASE_ONLY
@implementation AllocBase
+ (id)make:(Word)extra { return class_createInstance((Class)self,extra); }
- (id)identity { return self; }
- (id)choose:(id)other { return other ? other : self; }
- (Word)matches:(id)other { return self==other; }
- (Word)value:(Word)x { return _cmd==@selector(value:) ? x+5 : 1; }
@end
#endif
#if !INSTANCE_BASE_ONLY
@interface AllocChild : AllocBase
- (Word)value:(Word)x;
@end
@implementation AllocChild
- (Word)value:(Word)x { return [super value:x]+7; }
@end
extern Word protocol_abi_call(Word,Word,Word,Word *);
static Word captured[36];
extern long write(int,const void *,Word);
static id ctor_owned;
static int ctor_result;
static __thread Word current_epoch=17;
static int preserved(Word fn) {
    for (Word i=21;i<=28;i++) if (captured[i]!=200+i) return 1;
    if (captured[19]!=(Word)captured||captured[20]!=fn) return 2;
    if (captured[18]!=captured[32]||captured[29]!=captured[33]||captured[31]!=captured[34]||captured[30]!=captured[35]) return 3;
    return 0;
}
static int verify(void) {
    int result=0;id a=0,b=0,c=0,d=0;
    if (current_epoch!=23) return 9;
    current_epoch=41;
    Class cls=objc_getClass("AllocChild");
    if (!cls) {result=10;goto done;}
    if (class_createInstance((Class)0,64)) {result=11;goto done;}
    a=[AllocChild make:16];b=class_createInstance(cls,64);
    if (!a||!b||a==b) {result=12;goto done;}
    if ([(AllocBase *)a identity]!=a||[(AllocBase *)a matches:b]||![(AllocBase *)a matches:a]) {result=13;goto done;}
    if ([(AllocBase *)a choose:b]!=b||[(AllocBase *)a choose:(id)0]!=a||[(AllocBase *)(id)0 identity]) {result=14;goto done;}
    if ([(AllocChild *)a value:37]!=49) {result=15;goto done;}
    unsigned char *ea=(unsigned char *)a+sizeof(Class),*eb=(unsigned char *)b+sizeof(Class);
    for (Word i=0;i<16;i++) if (ea[i]||eb[i]) {result=16;goto done;}
    if (eb[63]) {result=17;goto done;}
    ea[0]=0xa5;ea[15]=0x5a;
    if (eb[0]||eb[15]||[(AllocBase *)a identity]!=a) {result=18;goto done;}
    if (object_dispose(a)) {a=0;result=19;goto done;}a=0;
    if (object_dispose((id)0)) {result=20;goto done;}
    c=class_createInstance(cls,16);if (!c) {result=21;goto done;}
    unsigned char *ec=(unsigned char *)c+sizeof(Class);
    for (Word i=0;i<16;i++) if (ec[i]) {result=22;goto done;}
    if ([(AllocChild *)c value:37]!=49) {result=23;goto done;}
    Word fn=(Word)class_createInstance;
    d=(id)protocol_abi_call(fn,(Word)cls,16,captured);
    if (!d||preserved(fn)||[(AllocBase *)d identity]!=d) {result=24;goto done;}
    fn=(Word)object_dispose;
    if (protocol_abi_call(fn,(Word)d,0,captured)||preserved(fn)) {d=0;result=25;goto done;}d=0;
done:
    if (a) object_dispose(a);
    if (b) object_dispose(b);
    if (c) object_dispose(c);
    if (d) object_dispose(d);
    return result;
}
#if INSTANCE_CTOR
__attribute__((constructor)) static void startup(void) {
    if (ctor_owned||ctor_result) {ctor_result=1;return;}
    if (current_epoch!=17) {ctor_result=1;return;}
    current_epoch=23;
    ctor_owned=[AllocChild make:16];
    if (!ctor_owned||[(AllocBase *)ctor_owned identity]!=ctor_owned||[(AllocChild *)ctor_owned value:37]!=49) {ctor_result=1;return;}
    unsigned char *extra=(unsigned char *)ctor_owned+sizeof(Class);
    for (Word i=0;i<16;i++) if (extra[i]) {ctor_result=1;return;}
    extra[0]=0x33;ctor_result=7;
}
#endif
int sable_main(void) {
#if INSTANCE_CTOR
    if (ctor_result!=7||!ctor_owned||*((unsigned char *)ctor_owned+sizeof(Class))!=0x33||[(AllocChild *)ctor_owned value:37]!=49) return 30;
    if (object_dispose(ctor_owned)) return 31;ctor_owned=0;
#else
    if (ctor_result||ctor_owned) return 32;
    if (current_epoch!=17) return 33;
    current_epoch=23;
#endif
    int result=verify();if (result) return result;
    if (current_epoch!=41) return 34;
    static const char message[]="Guest instances, pointer methods, zeroing, isolation and disposal OK\n";
    write(1,message,sizeof(message)-1);return 0;
}
#endif
