// SPDX-License-Identifier: MIT
// Original Objective-C declarations shared by the UI-profile providers.
#include "sable.h"
extern id class_createInstance(Class, Word);
extern id object_dispose(id);

__attribute__((objc_root_class))
@interface NSObject {
@public
    Class isa;
}
+ (id)alloc;
+ (id)new;
+ (Class)class;
- (id)init;
- (Class)class;
- (void)dealloc;
- (BOOL)respondsToSelector:(SEL)selector;
@end

// The raw isa word; Sable checks it against the allocation ledger.
static inline Class sable_isa(id object) { return *(Class *)(void *)object; }

@interface NSString : NSObject {
@public
    const char *bytes;
    Word length;
}
- (const char *)UTF8String;
- (Word)length;
@end

@interface NSSet : NSObject {
@public
    id object;
}
+ (id)sable_setWithObject:(id)object;
- (id)anyObject;
- (Word)count;
@end
