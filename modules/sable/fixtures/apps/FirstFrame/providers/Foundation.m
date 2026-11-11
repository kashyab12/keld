// SPDX-License-Identifier: MIT
// Original Foundation slice for the UI profile: NSString, NSStringFromClass and a
// one-object NSSet. Non-ARC; every object goes through the checked ownership services.
#include "sable-objc.h"

@implementation NSString
- (const char *)UTF8String { return bytes; }
- (Word)length { return length; }
@end

struct StringStorage {
    Class isa;
    const char *bytes;
    Word length;
    char text[];
};

NSString *NSStringFromClass(Class cls) {
    if (!cls) return nil;
    const char *name = class_getName(cls);
    Word n = 0;
    while (name[n]) n++;
    if (n > 63) return nil;
    struct StringStorage *s = (struct StringStorage *)class_createInstance([NSString class], n + 1);
    if (!s) return nil;
    for (Word i = 0; i <= n; i++) s->text[i] = name[i];
    s->bytes = s->text;
    s->length = n;
    return objc_autorelease((id)s);
}

@implementation NSSet
+ (id)sable_setWithObject:(id)value {
    NSSet *set = [[self alloc] init];
    set->object = objc_retain(value);
    return set;
}
- (id)anyObject { return object; }
- (Word)count { return object ? 1 : 0; }
- (void)dealloc {
    objc_release(object);
    [super dealloc];
}
@end
