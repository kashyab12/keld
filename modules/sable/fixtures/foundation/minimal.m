#import <Foundation/Foundation.h>
#include <unistd.h>
#include <string.h>
int main(void) {
    @autoreleasepool {
        NSString *value = [@"sable foundation" uppercaseString];
        const char *bytes = [value UTF8String];
        if (strcmp(bytes, "SABLE FOUNDATION")) return 2;
        if (write(1, bytes, strlen(bytes)) != 16 || write(1, "\n", 1) != 1) return 3;
    }
    return 0;
}
