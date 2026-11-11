// Explicit ordinary ownership control; the separate client is genuinely ARC.
#include "text.h"
extern void *objc_autoreleasePoolPush(void);
extern void objc_autoreleasePoolPop(void *);
extern id objc_retain(id), objc_retainAutorelease(id), objc_claimAutoreleasedReturnValue(id);
extern void objc_release(id);
extern void release_x20_control(id);
extern long write(int,const void *,Word);
static __thread Word marker=17;
static Word ctor;
__attribute__((constructor)) static void before(void) {
    void *p=objc_autoreleasePoolPush();
    id a=[SliceLife make:7];objc_release(a);
    objc_autoreleasePoolPop(p);
    marker=23;ctor=life_count();
}
int main(int argc,char **argv) {
    if(argc!=1 || !argv || !argv[0] || marker!=23 || ctor!=1)return 1;
    void *outer=objc_autoreleasePoolPush();
    id outside=[SliceLife make:1];objc_retainAutorelease(outside);objc_release(outside);
    void *inner=objc_autoreleasePoolPush();
    id a=[SliceLife make:2];objc_retainAutorelease(a);objc_release(a);id held=objc_retain(a);
    id b=[SliceLife make:3];objc_retainAutorelease(b);objc_release(b);
    objc_autoreleasePoolPop(inner);
    if(life_count()!=2 || life_order(1)!=3)return 2;
    objc_release(held);
    if(life_count()!=3 || life_order(2)!=2)return 3;
    inner=objc_autoreleasePoolPush();
    id f=[SliceLife make:4];id claim=objc_claimAutoreleasedReturnValue(f);
    objc_release(f);objc_autoreleasePoolPop(inner);
    if(life_count()!=3)return 4;
    objc_release(claim);
    if(life_order(3)!=4)return 5;
    id x=[SliceLife make:6];release_x20_control(x);
    if(life_order(4)!=6)return 6;
    objc_release((id)0);objc_retain((id)0);
    id literal=@"immortal";
    objc_retainAutorelease(literal);objc_release(literal);
    objc_autoreleasePoolPop(outer);
    if(life_count()!=6 || life_order(5)!=1 || marker!=23)return 7;
    static const char ok[]="Ownership, pools, guest dealloc, x20, ctor/TLV OK\n";
    return write(1,ok,sizeof(ok)-1)==sizeof(ok)-1?0:8;
}
