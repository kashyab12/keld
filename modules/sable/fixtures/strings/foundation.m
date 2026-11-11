#include "text.h"
@implementation NSString
- (const char *)UTF8String { return (const char *)bytesBits; }
@end
