// SPDX-License-Identifier: MIT
// DRAFT, COMPILE-ONLY. This is not the Contract 2 FirstFrame fixture and is never
// linked or run. It exists only so `scripts/fp-evidence.sh` can disassemble what
// Xcode clang emits for an ordinary small UIKit app (CGRect/CGPoint/CGFloat HFAs,
// UIColor component arguments, struct copies) to justify Sable's FP/SIMD subset.
#import <UIKit/UIKit.h>

@interface TapView : UIView
@end

@implementation TapView
- (instancetype)initWithFrame:(CGRect)frame {
    if ((self = [super initWithFrame:frame])) {
        self.backgroundColor = [UIColor colorWithRed:1 green:0 blue:0 alpha:1];
    }
    return self;
}
- (void)touchesBegan:(NSSet<UITouch *> *)touches withEvent:(UIEvent *)event {
    CGPoint point = [[touches anyObject] locationInView:self];
    CGRect bounds = self.bounds;
    if (point.x < CGRectGetMidX(bounds)) {
        self.backgroundColor = [UIColor colorWithRed:0 green:0 blue:1 alpha:1];
    } else {
        self.backgroundColor = [UIColor colorWithRed:0 green:1 blue:0 alpha:1];
    }
}
@end

@interface ViewController : UIViewController
@end

@implementation ViewController
- (void)loadView {
    CGRect frame = self.view.window ? self.view.window.bounds : UIScreen.mainScreen.bounds;
    self.view = [[TapView alloc] initWithFrame:CGRectInset(frame, 0, 0)];
}
- (void)viewDidLayoutSubviews {
    [super viewDidLayoutSubviews];
    CGRect bounds = self.view.bounds;
    CGFloat scale = floor(bounds.size.width / 2) + ceil(bounds.size.height / 3);
    self.view.layer.cornerRadius = scale > 0 ? fmin(scale, 8.0) : 0;
}
@end

@interface SceneDelegate : UIResponder <UIWindowSceneDelegate>
@property(nonatomic, strong) UIWindow *window;
@end

@implementation SceneDelegate
- (void)scene:(UIScene *)scene
    willConnectToSession:(UISceneSession *)session
                 options:(UISceneConnectionOptions *)options {
    UIWindowScene *windowScene = (UIWindowScene *)scene;
    self.window = [[UIWindow alloc] initWithWindowScene:windowScene];
    self.window.rootViewController = [ViewController new];
    [self.window makeKeyAndVisible];
}
@end

@interface AppDelegate : UIResponder <UIApplicationDelegate>
@end

@implementation AppDelegate
- (BOOL)application:(UIApplication *)application
    didFinishLaunchingWithOptions:(NSDictionary *)launchOptions {
    return YES;
}
@end

int main(int argc, char *argv[]) {
    @autoreleasepool {
        return UIApplicationMain(argc, argv, nil, NSStringFromClass([AppDelegate class]));
    }
}
