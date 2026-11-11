#include "text.h"
@interface SableConstantString : NSString @end
@implementation SableConstantString @end
// The class reference is the owning class itself, never a pointer cell.
__asm__(".globl ___CFConstantStringClassReference\n.set ___CFConstantStringClassReference, _OBJC_CLASS_$_SableConstantString");
