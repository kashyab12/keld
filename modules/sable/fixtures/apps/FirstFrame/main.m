// SPDX-License-Identifier: MIT
// Original FirstFrame fixture: the smallest ordinary UIKit scene app that draws a
// red frame, then turns blue (tap in the left half) or green (right half).
// Built with the iOS SDK and ARC; on Sable it runs against original providers.
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
    if (point.x < bounds.origin.x + bounds.size.width / 2) {
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
    // UIWindow resizes the root view to its own bounds when it is installed.
    self.view = [[TapView alloc] initWithFrame:CGRectMake(0, 0, 100, 100)];
}
@end

@interface SceneDelegate : UIResponder <UIWindowSceneDelegate>
@property(nonatomic, strong) UIWindow *window;
@end

@implementation SceneDelegate
- (void)scene:(UIScene *)scene
    willConnectToSession:(UISceneSession *)session
                 options:(UISceneConnectionOptions *)connectionOptions {
    self.window = [[UIWindow alloc] initWithWindowScene:(UIWindowScene *)scene];
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
    NSString *appDelegateClassName;
    @autoreleasepool {
        appDelegateClassName = NSStringFromClass([AppDelegate class]);
    }
    return UIApplicationMain(argc, argv, nil, appDelegateClassName);
}
