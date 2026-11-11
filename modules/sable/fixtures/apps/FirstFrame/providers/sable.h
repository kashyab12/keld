// SPDX-License-Identifier: MIT
// Original headerless declarations shared by the UI-profile providers.
typedef unsigned long Word;
typedef _Bool BOOL;
typedef double CGFloat;
typedef struct CGPoint { CGFloat x, y; } CGPoint;
typedef struct CGSize { CGFloat width, height; } CGSize;
typedef struct CGRect { CGPoint origin; CGSize size; } CGRect;
typedef id (*IMP)(id, SEL, ...);
#define nil ((id)0)
#define YES ((BOOL)1)
#define NO ((BOOL)0)
extern id objc_retain(id);
extern void objc_release(id);
extern id objc_autorelease(id);
extern void *objc_autoreleasePoolPush(void);
extern void objc_autoreleasePoolPop(void *);
extern BOOL class_respondsToSelector(Class, SEL);
extern const char *class_getName(Class);
extern id objc_getClass(const char *);
extern long write(int, const void *, Word);
extern void exit(int);
