// SPDX-License-Identifier: MIT
// Original root class and C entry points for the UI profile. Non-ARC, headerless.
#include "sable-objc.h"

extern IMP sable_class_cxx_destruct(Class, Class *next);

@implementation NSObject
+ (id)alloc { return class_createInstance(self, 0); }
+ (id)new { return [[self alloc] init]; }
+ (Class)class { return self; }
- (id)init { return self; }
- (Class)class { return sable_isa(self); }
- (BOOL)respondsToSelector:(SEL)selector { return class_respondsToSelector(sable_isa(self), selector); }
- (void)dealloc {
    // ARC .cxx_destruct for every class in the chain, most derived first.
    Class next = sable_isa(self);
    while (next) {
        IMP destruct = sable_class_cxx_destruct(next, &next);
        if (destruct) ((void (*)(id, SEL))destruct)(self, _cmd);
    }
    object_dispose(self);
}
@end

id objc_alloc(Class cls) { return cls ? [cls alloc] : nil; }
id objc_opt_new(Class cls) { return cls ? [cls new] : nil; }
Class objc_opt_class(id object) { return object ? [object class] : nil; }
void objc_storeStrong(id *location, id value) {
    id old = *location;
    if (value == old) return;
    objc_retain(value);
    *location = value;
    objc_release(old);
}
