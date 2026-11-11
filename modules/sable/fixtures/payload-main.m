// Original headerless Q payload/void setter and allocation-boundary probe.
typedef unsigned long Word;
extern Class objc_getClass(const char *);
extern id class_createInstance(Class,Word);
extern id object_dispose(id);
#define PAYLOAD_SIZE (8+8*(ROOT_WORDS+CHILD_WORDS))
#define CHILD_SEED (ROOT_WORDS ? 25 : 42)
__attribute__((objc_root_class))
@interface PayloadBase { Class isa;
#if ROOT_WORDS>=1
Word baseValue;
#endif
#if ROOT_WORDS>=2
Word base2;
#endif
#if ROOT_WORDS>=3
Word base3;
#endif
#if ROOT_WORDS>=4
Word base4;
#endif
#if ROOT_WORDS>=5
Word base5;
#endif
#if ROOT_WORDS>=6
Word base6;
#endif
#if ROOT_WORDS>=7
Word base7;
#endif
}
#if ROOT_WORDS
- (void)setBase:(Word)value;
#endif
- (Word)base;
- (Word)sum;
@end
#if !PAYLOAD_SPLIT || PAYLOAD_BASE_ONLY
@implementation PayloadBase
#if ROOT_WORDS
- (void)setBase:(Word)value {
#if ROOT_WORDS>=1
baseValue=value+0;
#endif
#if ROOT_WORDS>=2
base2=value+1;
#endif
#if ROOT_WORDS>=3
base3=value+2;
#endif
#if ROOT_WORDS>=4
base4=value+3;
#endif
#if ROOT_WORDS>=5
base5=value+4;
#endif
#if ROOT_WORDS>=6
base6=value+5;
#endif
#if ROOT_WORDS>=7
base7=value+6;
#endif
}
#endif
- (Word)base {
#if ROOT_WORDS
return baseValue;
#else
return 0;
#endif
}
- (Word)sum { Word result=0;
#if ROOT_WORDS>=1
result+=baseValue;
#endif
#if ROOT_WORDS>=2
result+=base2;
#endif
#if ROOT_WORDS>=3
result+=base3;
#endif
#if ROOT_WORDS>=4
result+=base4;
#endif
#if ROOT_WORDS>=5
result+=base5;
#endif
#if ROOT_WORDS>=6
result+=base6;
#endif
#if ROOT_WORDS>=7
result+=base7;
#endif
return result; }
@end
#endif
#if !PAYLOAD_BASE_ONLY
@interface PayloadChild : PayloadBase {
#if CHILD_WORDS>=1
Word childValue;
#endif
#if CHILD_WORDS>=2
Word child2;
#endif
#if CHILD_WORDS>=3
Word child3;
#endif
#if CHILD_WORDS>=4
Word child4;
#endif
#if CHILD_WORDS>=5
Word child5;
#endif
#if CHILD_WORDS>=6
Word child6;
#endif
}
#if CHILD_WORDS
- (void)setChild:(Word)value;
#endif
- (Word)child;
- (Word)sum;
@end
@implementation PayloadChild
#if CHILD_WORDS
- (void)setChild:(Word)value {
#if CHILD_WORDS>=1
childValue=value+0;
#endif
#if CHILD_WORDS>=2
child2=value+1;
#endif
#if CHILD_WORDS>=3
child3=value+2;
#endif
#if CHILD_WORDS>=4
child4=value+3;
#endif
#if CHILD_WORDS>=5
child5=value+4;
#endif
#if CHILD_WORDS>=6
child6=value+5;
#endif
}
#endif
- (Word)child {
#if CHILD_WORDS
return childValue;
#else
return 0;
#endif
}
- (Word)sum { Word result=[super sum];
#if CHILD_WORDS>=1
result+=childValue;
#endif
#if CHILD_WORDS>=2
result+=child2;
#endif
#if CHILD_WORDS>=3
result+=child3;
#endif
#if CHILD_WORDS>=4
result+=child4;
#endif
#if CHILD_WORDS>=5
result+=child5;
#endif
#if CHILD_WORDS>=6
result+=child6;
#endif
return result; }
@end
extern long write(int,const void *,Word);
static id ctor_owned;
static int ctor_result;
static __thread Word epoch=17;
static Word expected(Word a,Word b) {
    Word sum=0;
    for (Word i=0;i<ROOT_WORDS;i++) sum+=a+i;
    for (Word i=0;i<CHILD_WORDS;i++) sum+=b+i;
    return sum;
}
static void assign(PayloadChild *object,Word a,Word b) {
#if ROOT_WORDS
    [object setBase:a];
#endif
#if CHILD_WORDS
    [object setChild:b];
#endif
}
static int pristine(id object,Word extra) {
    Word *data=(Word *)object;
    for (Word i=1;i<PAYLOAD_SIZE/8;i++) if (data[i]) return 1;
    unsigned char *e=(unsigned char *)object+PAYLOAD_SIZE;
    for (Word i=0;i<extra;i++) if (e[i]) return 2;
    return 0;
}
static int verify(void) {
    int result=0;id a=0,b=0,c=0;
    Class cls=objc_getClass("PayloadChild");if (!cls||epoch!=23) return 10;
    epoch=41;
    a=class_createInstance(cls,64);b=class_createInstance(cls,16);
    if (!a||!b||a==b||pristine(a,64)||pristine(b,16)) {result=11;goto done;}
    assign((PayloadChild *)a,17,CHILD_SEED);
    if ([(PayloadChild *)a sum]!=expected(17,CHILD_SEED)) {result=12;goto done;}
    Word *data=(Word *)a;
    for (Word i=0;i<ROOT_WORDS;i++) if (data[1+i]!=17+i) {result=13;goto done;}
    for (Word i=0;i<CHILD_WORDS;i++) if (data[1+ROOT_WORDS+i]!=CHILD_SEED+i) {result=14;goto done;}
    if ([(PayloadChild *)b base]||[(PayloadChild *)b child]||[(PayloadChild *)b sum]||pristine(b,16)) {result=15;goto done;}
    unsigned char *ea=(unsigned char *)a+PAYLOAD_SIZE,*eb=(unsigned char *)b+PAYLOAD_SIZE;
    ea[0]=0xa5;ea[63]=0x5a;
    if (eb[0]||eb[15]||[(PayloadChild *)a sum]!=expected(17,CHILD_SEED)) {result=16;goto done;}
    if (object_dispose(a)) {a=0;result=17;goto done;}a=0;
    c=class_createInstance(cls,64);
    if (!c||pristine(c,64)||[(PayloadChild *)c sum]) {result=18;goto done;}
    assign((PayloadChild *)c,31,7);
    if ([(PayloadChild *)c sum]!=expected(31,7)||[(PayloadChild *)b sum]) result=19;
done:
    if(a)object_dispose(a);if(b)object_dispose(b);if(c)object_dispose(c);
    return result;
}
#if PAYLOAD_CTOR
__attribute__((constructor)) static void startup(void) {
    if (ctor_owned||ctor_result||epoch!=17) {ctor_result=1;return;}
    epoch=23;
    ctor_owned=class_createInstance(objc_getClass("PayloadChild"),64);
    if (!ctor_owned||pristine(ctor_owned,64)) {ctor_result=1;return;}
    assign((PayloadChild *)ctor_owned,5,7);
    *((unsigned char *)ctor_owned+PAYLOAD_SIZE)=0x33;
    ctor_result=7;
}
#endif
int sable_main(void) {
#if PAYLOAD_CTOR
    if (ctor_result!=7||!ctor_owned||epoch!=23||[(PayloadChild *)ctor_owned sum]!=expected(5,7)||*((unsigned char *)ctor_owned+PAYLOAD_SIZE)!=0x33) return 30;
    if(object_dispose(ctor_owned))return 31;ctor_owned=0;
#else
    if(ctor_owned||ctor_result||epoch!=17)return 32;
    epoch=23;
#endif
    int result=verify();if(result)return result;
    if(epoch!=41)return 33;
    static const char message[]="Payload fields, void setters, super sums, extra bounds and reset OK\n";
    write(1,message,sizeof(message)-1);return 0;
}
#endif
