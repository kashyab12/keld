// Original MIT test app. Uses the installed Apple SDK only for local native builds.
@import UIKit;
@import MetalKit;

static NSString *const StoreKey = @"sable-notes";

@interface MetalController : UIViewController <MTKViewDelegate>
@property MTKView *canvas;
@property UILabel *status;
@property BOOL blue;
@end

@implementation MetalController
- (void)viewDidLoad {
    [super viewDidLoad];
    self.title = @"Metal pixels";
    self.view.backgroundColor = UIColor.systemBackgroundColor;
    id<MTLDevice> device = MTLCreateSystemDefaultDevice();
    self.status = [[UILabel alloc] initWithFrame:CGRectMake(20, 130, 350, 100)];
    self.status.numberOfLines = 0;
    self.status.accessibilityIdentifier = @"gpu-status";
    self.status.text = device ? [@"Waiting: " stringByAppendingString:device.name] : @"GPU unavailable";
    [self.view addSubview:self.status];
    self.canvas = [[MTKView alloc] initWithFrame:CGRectMake(30, 260, 300, 240) device:device];
    self.canvas.accessibilityIdentifier = @"metal-canvas";
    self.canvas.isAccessibilityElement = YES;
    self.canvas.accessibilityLabel = @"Metal output";
    self.canvas.framebufferOnly = NO;
    self.canvas.colorPixelFormat = MTLPixelFormatBGRA8Unorm;
    self.canvas.paused = YES;
    self.canvas.enableSetNeedsDisplay = YES;
    self.canvas.delegate = self;
    [self.view addSubview:self.canvas];
    UIButton *toggle = [UIButton buttonWithType:UIButtonTypeSystem];
    toggle.frame = CGRectMake(30, 530, 280, 50);
    [toggle setTitle:@"Toggle color" forState:UIControlStateNormal];
    toggle.accessibilityIdentifier = @"toggle-color";
    [toggle addTarget:self action:@selector(toggle) forControlEvents:UIControlEventTouchUpInside];
    [self.view addSubview:toggle];
    [self.canvas setNeedsDisplay];
}
- (void)toggle { self.blue = !self.blue; [self.canvas setNeedsDisplay]; }
- (void)mtkView:(MTKView *)view drawableSizeWillChange:(CGSize)size {}
- (void)drawInMTKView:(MTKView *)view {
    id<CAMetalDrawable> drawable = view.currentDrawable;
    MTLRenderPassDescriptor *pass = view.currentRenderPassDescriptor;
    if (!drawable || !pass) { self.status.text = @"No drawable"; return; }
    pass.colorAttachments[0].loadAction = MTLLoadActionClear;
    pass.colorAttachments[0].storeAction = MTLStoreActionStore;
    pass.colorAttachments[0].clearColor = self.blue ? MTLClearColorMake(0, 0, 1, 1) : MTLClearColorMake(1, 0, 0, 1);
    id<MTLCommandQueue> queue = [view.device newCommandQueue];
    id<MTLCommandBuffer> commands = [queue commandBuffer];
    id<MTLRenderCommandEncoder> encoder = [commands renderCommandEncoderWithDescriptor:pass];
    [encoder endEncoding];
    // Copy a real rendered pixel back from the same drawable that is presented.
    id<MTLBuffer> pixel = [view.device newBufferWithLength:256 options:MTLResourceStorageModeShared];
    id<MTLBlitCommandEncoder> copy = [commands blitCommandEncoder];
    [copy copyFromTexture:drawable.texture sourceSlice:0 sourceLevel:0
             sourceOrigin:MTLOriginMake(10, 10, 0) sourceSize:MTLSizeMake(1, 1, 1)
                 toBuffer:pixel destinationOffset:0 destinationBytesPerRow:256 destinationBytesPerImage:256];
    [copy endEncoding];
    [commands presentDrawable:drawable];
    [commands commit];
    [commands waitUntilCompleted];
    if (commands.status != MTLCommandBufferStatusCompleted) {
        self.status.text = [@"GPU failed: " stringByAppendingString:commands.error.description ?: @"unknown"];
        return;
    }
    const unsigned char *p = pixel.contents;
    self.status.text = [NSString stringWithFormat:@"RGBA %u,%u,%u,%u | %@", p[2], p[1], p[0], p[3], view.device.name];
}
@end

@interface DetailController : UIViewController
@property NSString *item;
@property UITextView *editor;
@property UILabel *status;
@end

@implementation DetailController
- (void)viewDidLoad {
    [super viewDidLoad];
    self.title = self.item;
    self.view.backgroundColor = UIColor.systemBackgroundColor;
    self.editor = [[UITextView alloc] initWithFrame:CGRectMake(20, 130, 330, 200)];
    self.editor.font = [UIFont systemFontOfSize:20];
    self.editor.layer.borderWidth = 1;
    self.editor.accessibilityIdentifier = @"note-editor";
    self.editor.autocorrectionType = UITextAutocorrectionTypeNo;
    self.editor.smartQuotesType = UITextSmartQuotesTypeNo;
    self.editor.text = [[NSUserDefaults.standardUserDefaults dictionaryForKey:StoreKey] objectForKey:self.item] ?: @"";
    [self.view addSubview:self.editor];
    UIButton *save = [UIButton buttonWithType:UIButtonTypeSystem];
    save.frame = CGRectMake(20, 350, 330, 44);
    [save setTitle:@"Save note" forState:UIControlStateNormal];
    save.accessibilityIdentifier = @"save-note";
    [save addTarget:self action:@selector(save) forControlEvents:UIControlEventTouchUpInside];
    [self.view addSubview:save];
    self.status = [[UILabel alloc] initWithFrame:CGRectMake(20, 400, 330, 50)];
    self.status.accessibilityIdentifier = @"save-status";
    [self.view addSubview:self.status];
}
- (void)save {
    NSMutableDictionary *notes = [[NSUserDefaults.standardUserDefaults dictionaryForKey:StoreKey] mutableCopy] ?: NSMutableDictionary.new;
    notes[self.item] = self.editor.text;
    [NSUserDefaults.standardUserDefaults setObject:notes forKey:StoreKey];
    [self.editor resignFirstResponder];
    self.status.text = @"Note saved";
}
@end

@interface NotesController : UIViewController <UITableViewDataSource, UITableViewDelegate, UITextFieldDelegate>
@property UITableView *table;
@property UITextField *search;
@property UILabel *count;
@property NSArray<NSString *> *items;
@end

@implementation NotesController
- (void)viewDidLoad {
    [super viewDidLoad];
    self.title = @"Sable Notes";
    self.view.backgroundColor = UIColor.systemBackgroundColor;
    self.search = [[UITextField alloc] initWithFrame:CGRectMake(20, 120, 330, 44)];
    self.search.borderStyle = UITextBorderStyleRoundedRect;
    self.search.placeholder = @"Search items";
    self.search.accessibilityIdentifier = @"search-items";
    self.search.autocorrectionType = UITextAutocorrectionTypeNo;
    self.search.delegate = self;
    [self.search addTarget:self action:@selector(refresh) forControlEvents:UIControlEventEditingChanged];
    [self.view addSubview:self.search];
    self.count = [[UILabel alloc] initWithFrame:CGRectMake(20, 175, 130, 44)];
    self.count.accessibilityIdentifier = @"saved-count";
    [self.view addSubview:self.count];
    UIButton *reset = [UIButton buttonWithType:UIButtonTypeSystem];
    reset.frame = CGRectMake(150, 175, 80, 44);
    [reset setTitle:@"Reset" forState:UIControlStateNormal];
    reset.accessibilityIdentifier = @"reset-notes";
    [reset addTarget:self action:@selector(reset) forControlEvents:UIControlEventTouchUpInside];
    [self.view addSubview:reset];
    UIButton *gpu = [UIButton buttonWithType:UIButtonTypeSystem];
    gpu.frame = CGRectMake(240, 175, 100, 44);
    [gpu setTitle:@"Metal test" forState:UIControlStateNormal];
    gpu.accessibilityIdentifier = @"open-metal";
    [gpu addTarget:self action:@selector(openMetal) forControlEvents:UIControlEventTouchUpInside];
    [self.view addSubview:gpu];
    self.table = [[UITableView alloc] initWithFrame:CGRectMake(0, 230, self.view.bounds.size.width, self.view.bounds.size.height - 230) style:UITableViewStylePlain];
    self.table.autoresizingMask = UIViewAutoresizingFlexibleWidth | UIViewAutoresizingFlexibleHeight;
    self.table.dataSource = self;
    self.table.delegate = self;
    self.table.accessibilityIdentifier = @"items-table";
    [self.view addSubview:self.table];
    [self refresh];
}
- (void)viewWillAppear:(BOOL)animated { [super viewWillAppear:animated]; [self refresh]; }
- (BOOL)textFieldShouldReturn:(UITextField *)textField { [textField resignFirstResponder]; return YES; }
- (void)refresh {
    NSMutableArray *items = NSMutableArray.new;
    for (int i = 0; i < 60; i++) {
        NSString *item = [NSString stringWithFormat:@"Item %02d", i];
        if (!self.search.text.length || [item localizedCaseInsensitiveContainsString:self.search.text]) [items addObject:item];
    }
    self.items = items;
    self.count.text = [NSString stringWithFormat:@"Saved: %lu", (unsigned long)[NSUserDefaults.standardUserDefaults dictionaryForKey:StoreKey].count];
    [self.table reloadData];
}
- (void)reset { [NSUserDefaults.standardUserDefaults removeObjectForKey:StoreKey]; [self refresh]; }
- (void)openMetal { [self.navigationController pushViewController:MetalController.new animated:YES]; }
- (NSInteger)tableView:(UITableView *)tableView numberOfRowsInSection:(NSInteger)section { return self.items.count; }
- (UITableViewCell *)tableView:(UITableView *)tableView cellForRowAtIndexPath:(NSIndexPath *)indexPath {
    UITableViewCell *cell = [[UITableViewCell alloc] initWithStyle:UITableViewCellStyleDefault reuseIdentifier:nil];
    cell.textLabel.text = self.items[indexPath.row];
    cell.accessibilityIdentifier = self.items[indexPath.row];
    return cell;
}
- (void)tableView:(UITableView *)tableView didSelectRowAtIndexPath:(NSIndexPath *)indexPath {
    DetailController *detail = DetailController.new;
    detail.item = self.items[indexPath.row];
    [self.navigationController pushViewController:detail animated:YES];
}
@end

@interface SceneDelegate : UIResponder <UIWindowSceneDelegate>
@property(nonatomic, strong) UIWindow *window;
@end
@implementation SceneDelegate
- (void)scene:(UIScene *)scene willConnectToSession:(UISceneSession *)session options:(UISceneConnectionOptions *)options {
    self.window = [[UIWindow alloc] initWithWindowScene:(UIWindowScene *)scene];
    self.window.rootViewController = [[UINavigationController alloc] initWithRootViewController:NotesController.new];
    [self.window makeKeyAndVisible];
}
@end
@interface AppDelegate : UIResponder <UIApplicationDelegate>
@end
@implementation AppDelegate
@end
int main(int argc, char *argv[]) {
    @autoreleasepool { return UIApplicationMain(argc, argv, nil, NSStringFromClass(AppDelegate.class)); }
}
