// Original narrow declarations; no SDK headers or Apple implementation.
typedef unsigned long Word;
__attribute__((objc_root_class))
@interface NSString { Class isa; Word flags; Word bytesBits; Word count; }
- (const char *)UTF8String;
@end
