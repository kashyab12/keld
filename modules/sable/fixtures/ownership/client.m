#include "text.h"
extern Word strlen(const char *);
extern int strcmp(const char *,const char *);
extern long write(int,const void *,Word);
int main(void) {
    @autoreleasepool {
        NSString *value=[@"sable foundation" uppercaseString];
        const char *bytes=[value UTF8String];
        if(strcmp(bytes,"SABLE FOUNDATION"))return 2;
        if(write(1,bytes,strlen(bytes))!=16 || write(1,"\n",1)!=1)return 3;
    }
    if(string_destroyed()!=1)return 4;
    @autoreleasepool {
        NSString *empty=[@"" uppercaseString];
        if(strlen([empty UTF8String]))return 5;
        NSString *edge=[@"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789!" uppercaseString];
        const char *bytes=[edge UTF8String];
        if(strlen(bytes)!=63 || strcmp(bytes,"ABCDEFGHIJKLMNOPQRSTUVWXYZABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789!"))return 6;
        NSString *different=[@"aZ09!?xy" uppercaseString];
        if(strcmp([different UTF8String],"AZ09!?XY"))return 7;
        if(strcmp([@"sable foundation" UTF8String],"sable foundation"))return 8;
    }
    return string_destroyed()==4?0:9;
}
