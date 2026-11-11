#include "text.h"
extern id literal_same(void),literal_other(void);
extern Word strlen(const char *);
extern int strcmp(const char *,const char *);
extern long write(int,const void *,Word);
static Word ctor_seen;
static int check(void) {
    NSString *a=@"sable foundation",*same=literal_same(),*other=literal_other();
    if (a==same || same!=literal_same() || other!=literal_other()) return 1;
    const char *x=[a UTF8String],*y=[same UTF8String],*z=[other UTF8String];
    if (strcmp(x,"sable foundation") || strcmp(y,x) || strcmp(z,"dylib Alpha09")) return 2;
    if (strlen(x)!=16 || strlen(z)!=13) return 3;
    if (strcmp(x,z)<=0 || strcmp(z,x)>=0 || strcmp("abc","abcd")>=0 || strcmp("abcd","abc")<=0) return 4;
    NSString *empty=@"",*limit=@"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789!";
    if (strlen([empty UTF8String]) || strcmp([empty UTF8String],"")) return 5;
    const char *bytes=[limit UTF8String];
    if (strlen(bytes)!=63 || strcmp(bytes,"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789!")) return 6;
    return 0;
}
#if STRING_CTOR
__attribute__((constructor)) static void startup(void) { ctor_seen=check()?99:23; }
#endif
int sable_main(void) {
#if STRING_CTOR
    if (ctor_seen!=23) return 10;
#else
    if (ctor_seen) return 11;
#endif
    int status=check();if(status)return status;
    static const char out[]="Constant strings, guest UTF8String and libc checks OK\n";
    return write(1,out,sizeof(out)-1)==sizeof(out)-1?0:12;
}
