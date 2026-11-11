// SPDX-License-Identifier: MIT
// Original UIKit slice for Sable's fixed UI profile: 390x844 points at scale 1.
// Non-ARC and headerless. It creates the application and scene, runs the guest
// view tree, fills a BGRA8 framebuffer in guest memory and dispatches touches.
// The host only copies presented frames out and delivers external input events.
#include "sable-objc.h"

#define WIDTH 390
#define HEIGHT 844

// Negative-control builds only (scripts/build-firstframe.sh): present an
// unmapped framebuffer range, or omit the UIApplicationMain export.
#ifdef SABLE_FORGED_FRAMEBUFFER
#define PRESENTED ((const char *)framebuffer + 0x40000000)
#else
#define PRESENTED framebuffer
#endif
#ifdef SABLE_MISSING_EXPORT
#define UIApplicationMain SableNegativeControlApplicationMain
#endif

// Host services (checked SVC stubs in UIKit-services.s).
extern Word sable_ui_launch_info(char *buffer, Word capacity);
extern void sable_ui_present(const void *pixels, Word width, Word height, Word stride);
extern unsigned sable_ui_wait_event(void *event);

struct SableEvent {
    unsigned kind; // 1 = touch began, 2 = touch ended, 3 = quit
    unsigned sequence;
    double x, y;
    Word reserved;
};

static inline CGRect rect(CGFloat x, CGFloat y, CGFloat w, CGFloat h) {
    CGRect r = {{x, y}, {w, h}};
    return r;
}

@interface UIResponder : NSObject
- (id)nextResponder;
- (void)touchesBegan:(id)touches withEvent:(id)event;
@end

@interface UIColor : NSObject {
@public
    CGFloat red, green, blue, alpha;
}
+ (id)colorWithRed:(CGFloat)r green:(CGFloat)g blue:(CGFloat)b alpha:(CGFloat)a;
- (id)initWithRed:(CGFloat)r green:(CGFloat)g blue:(CGFloat)b alpha:(CGFloat)a;
@end

@interface UIView : UIResponder {
@public
    CGRect frame;
    UIColor *backgroundColor;
    UIView *superview;
    id viewController;
    UIView *subviews[8];
    Word subviewCount;
}
- (id)initWithFrame:(CGRect)frame;
- (CGRect)frame;
- (void)setFrame:(CGRect)frame;
- (CGRect)bounds;
- (UIColor *)backgroundColor;
- (void)setBackgroundColor:(UIColor *)color;
- (void)addSubview:(UIView *)view;
- (UIView *)superview;
- (void)layoutSubviews;
- (UIView *)hitTest:(CGPoint)point withEvent:(id)event;
@end

@interface UIViewController : UIResponder {
@public
    UIView *view;
}
- (UIView *)view;
- (void)setView:(UIView *)view;
- (void)loadView;
- (void)viewDidLoad;
- (void)viewDidLayoutSubviews;
@end

@interface UIWindow : UIView {
@public
    UIViewController *rootViewController;
    id windowScene;
}
- (id)initWithWindowScene:(id)scene;
- (UIViewController *)rootViewController;
- (void)setRootViewController:(UIViewController *)controller;
- (void)makeKeyAndVisible;
@end

@interface UIScene : UIResponder {
@public
    id delegate;
}
- (id)delegate;
- (void)setDelegate:(id)delegate;
@end
@interface UIWindowScene : UIScene
@end
@interface UISceneSession : NSObject
@end
@interface UIApplication : UIResponder {
@public
    id delegate;
}
- (id)delegate;
- (void)setDelegate:(id)delegate;
@end
@interface UITouch : NSObject {
@public
    CGPoint location;
    UIView *view;
    UIWindow *window;
}
- (CGPoint)locationInView:(UIView *)view;
- (UIView *)view;
- (UIWindow *)window;
@end
@interface UIEvent : NSObject
@end

// Optional delegate entry points, declared so the selectors exist here too.
@interface NSObject (SableUIDelegates)
- (BOOL)application:(id)application didFinishLaunchingWithOptions:(id)options;
- (void)scene:(id)scene willConnectToSession:(id)session options:(id)options;
@end

static UIWindow *keyWindow;

@implementation UIResponder
- (id)nextResponder { return nil; }
- (void)touchesBegan:(id)touches withEvent:(id)event {
    [[self nextResponder] touchesBegan:touches withEvent:event];
}
@end

@implementation UIColor
+ (id)colorWithRed:(CGFloat)r green:(CGFloat)g blue:(CGFloat)b alpha:(CGFloat)a {
    return objc_autorelease([[self alloc] initWithRed:r green:g blue:b alpha:a]);
}
- (id)initWithRed:(CGFloat)r green:(CGFloat)g blue:(CGFloat)b alpha:(CGFloat)a {
    if ((self = [super init])) {
        red = r;
        green = g;
        blue = b;
        alpha = a;
    }
    return self;
}
@end

@implementation UIView
- (id)init { return [self initWithFrame:rect(0, 0, 0, 0)]; }
- (id)initWithFrame:(CGRect)value {
    if ((self = [super init])) frame = value;
    return self;
}
- (CGRect)frame { return frame; }
- (void)setFrame:(CGRect)value { frame = value; }
- (CGRect)bounds { return rect(0, 0, frame.size.width, frame.size.height); }
- (UIColor *)backgroundColor { return backgroundColor; }
- (void)setBackgroundColor:(UIColor *)color {
    objc_retain(color);
    objc_release(backgroundColor);
    backgroundColor = color;
}
- (void)addSubview:(UIView *)child {
    if (!child || child->superview == self || subviewCount == 8) return;
    subviews[subviewCount++] = objc_retain(child);
    child->superview = self;
}
- (UIView *)superview { return superview; }
- (id)nextResponder { return viewController ? viewController : superview; }
- (void)layoutSubviews {}
- (UIView *)hitTest:(CGPoint)point withEvent:(id)event {
    if (point.x < 0 || point.y < 0 || point.x >= frame.size.width || point.y >= frame.size.height)
        return nil;
    for (Word i = subviewCount; i-- > 0;) {
        UIView *child = subviews[i];
        CGPoint local = {point.x - child->frame.origin.x, point.y - child->frame.origin.y};
        UIView *hit = [child hitTest:local withEvent:event];
        if (hit) return hit;
    }
    return self;
}
- (void)dealloc {
    for (Word i = 0; i < subviewCount; i++) objc_release(subviews[i]);
    objc_release(backgroundColor);
    [super dealloc];
}
@end

@implementation UIViewController
- (UIView *)view {
    if (!view) {
        [self loadView];
        [self viewDidLoad];
    }
    return view;
}
- (void)setView:(UIView *)value {
    objc_retain(value);
    if (view) view->viewController = nil;
    objc_release(view);
    view = value;
    if (view) view->viewController = self;
}
- (void)loadView {
    [self setView:objc_autorelease([[UIView alloc] initWithFrame:rect(0, 0, WIDTH, HEIGHT)])];
}
- (void)viewDidLoad {}
- (void)viewDidLayoutSubviews {}
- (id)nextResponder { return view ? view->superview : nil; }
- (void)dealloc {
    objc_release(view);
    [super dealloc];
}
@end

@implementation UIWindow
- (id)initWithWindowScene:(id)scene {
    if ((self = [self initWithFrame:rect(0, 0, WIDTH, HEIGHT)])) windowScene = scene;
    return self;
}
- (UIViewController *)rootViewController { return rootViewController; }
- (void)setRootViewController:(UIViewController *)controller {
    objc_retain(controller);
    objc_release(rootViewController);
    rootViewController = controller;
    UIView *root = [controller view];
    [root setFrame:[self bounds]];
    [self addSubview:root];
}
- (void)makeKeyAndVisible {
    objc_retain(self);
    objc_release(keyWindow);
    keyWindow = self;
}
- (id)nextResponder { return nil; }
@end

@implementation UIScene
- (id)delegate { return delegate; }
- (void)setDelegate:(id)value {
    objc_retain(value);
    objc_release(delegate);
    delegate = value;
}
@end
@implementation UIWindowScene
@end
@implementation UISceneSession
@end
@implementation UIApplication
- (id)delegate { return delegate; }
- (void)setDelegate:(id)value {
    objc_retain(value);
    objc_release(delegate);
    delegate = value;
}
@end
@implementation UITouch
- (CGPoint)locationInView:(UIView *)target {
    CGPoint p = location;
    for (UIView *v = target; v && v != (UIView *)window; v = v->superview) {
        p.x -= v->frame.origin.x;
        p.y -= v->frame.origin.y;
    }
    return p;
}
- (UIView *)view { return view; }
- (UIWindow *)window { return window; }
@end
@implementation UIEvent
@end

// Software compositor: the guest computes every pixel of the BGRA8 frame.
static unsigned framebuffer[WIDTH * HEIGHT] __attribute__((aligned(16)));

static unsigned channel(CGFloat c) {
    if (!(c > 0)) return 0;
    if (c >= 1) return 255;
    return (unsigned)(c * 255.0 + 0.5);
}

static long clamp(CGFloat v, long limit) {
    long n = (long)__builtin_floor(v + 0.5);
    return n < 0 ? 0 : n > limit ? limit : n;
}

static void fill(CGRect r, unsigned bgra) {
    long x0 = clamp(r.origin.x, WIDTH), x1 = clamp(r.origin.x + r.size.width, WIDTH);
    long y0 = clamp(r.origin.y, HEIGHT), y1 = clamp(r.origin.y + r.size.height, HEIGHT);
    unsigned long pair = (unsigned long)bgra << 32 | bgra;
    for (long y = y0; y < y1; y++) {
        unsigned *row = framebuffer + y * WIDTH;
        long x = x0;
        if (x < x1 && (x & 1)) row[x++] = bgra;
        for (; x + 1 < x1; x += 2) *(unsigned long *)(row + x) = pair;
        if (x < x1) row[x] = bgra;
    }
}

static void render(UIView *v, CGFloat ox, CGFloat oy) {
    CGFloat x = ox + v->frame.origin.x, y = oy + v->frame.origin.y;
    UIColor *c = v->backgroundColor;
    if (c && c->alpha > 0) {
        unsigned bgra = 0xff000000u | channel(c->red) << 16 | channel(c->green) << 8 | channel(c->blue);
        fill(rect(x, y, v->frame.size.width, v->frame.size.height), bgra);
    }
    for (Word i = 0; i < v->subviewCount; i++) render(v->subviews[i], x, y);
}

static void layout(UIView *v) {
    [v layoutSubviews];
    for (Word i = 0; i < v->subviewCount; i++) layout(v->subviews[i]);
    if (v->viewController) [v->viewController viewDidLayoutSubviews];
}

static UIApplication *application;

int UIApplicationMain(int argc, char **argv, NSString *principal, NSString *delegateName) {
    (void)argc;
    (void)argv;
    (void)principal;
    void *pool = objc_autoreleasePoolPush();
    application = [UIApplication new];
    Class delegateClass = delegateName ? (Class)objc_getClass([delegateName UTF8String]) : nil;
    if (delegateName && !delegateClass) exit(121);
    id appDelegate = delegateClass ? [delegateClass new] : nil;
    [application setDelegate:appDelegate];
    objc_release(appDelegate);
    if ([appDelegate respondsToSelector:@selector(application:didFinishLaunchingWithOptions:)] &&
        ![appDelegate application:application didFinishLaunchingWithOptions:nil])
        exit(122);
    char name[64];
    Word n = sable_ui_launch_info(name, sizeof name);
    Class sceneDelegateClass = n ? (Class)objc_getClass(name) : nil;
    if (!sceneDelegateClass) exit(123);
    UIWindowScene *scene = [UIWindowScene new];
    UISceneSession *session = [UISceneSession new];
    id sceneDelegate = [sceneDelegateClass new];
    [scene setDelegate:sceneDelegate];
    objc_release(sceneDelegate);
    if ([sceneDelegate respondsToSelector:@selector(scene:willConnectToSession:options:)])
        [sceneDelegate scene:scene willConnectToSession:session options:nil];
    objc_autoreleasePoolPop(pool);

    // Persistent provider-owned input objects: one touch, one set, one event.
    UITouch *touch = [UITouch new];
    NSSet *touches = [NSSet sable_setWithObject:touch];
    UIEvent *event = [UIEvent new];
    struct SableEvent input;
    for (;;) {
        pool = objc_autoreleasePoolPush();
        fill(rect(0, 0, WIDTH, HEIGHT), 0xff000000u);
        if (keyWindow) {
            layout(keyWindow);
            render(keyWindow, 0, 0);
        }
        sable_ui_present(PRESENTED, WIDTH, HEIGHT, WIDTH * 4);
        unsigned kind = sable_ui_wait_event(&input);
        if (kind == 3) exit(0);
        if (kind == 1 && keyWindow) {
            CGPoint point = {input.x, input.y};
            UIView *target = [keyWindow hitTest:point withEvent:event];
            if (target) {
                touch->location = point;
                touch->view = target;
                touch->window = keyWindow;
                [target touchesBegan:touches withEvent:event];
            }
        }
        objc_autoreleasePoolPop(pool);
    }
}
