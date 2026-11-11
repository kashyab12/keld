# FirstFrame: an SDK-compiled UIKit scene app on Sable

`modules/sable/fixtures/apps/FirstFrame/main.m` is an original MIT scene app:
AppDelegate and SceneDelegate (scene manifest in `Info.plist`, committed binary
form `Info.bplist`), a ViewController whose `loadView` creates a TapView, and a
TapView that starts red and turns blue (tap in the left half) or green (right
half). It is built with Xcode clang and ARC at `-O0` and `-Os`. The SDK-linked
binary stays in ignored `artifacts/` and is never committed.

On Sable it runs against original headerless providers in `providers/`, built by
`scripts/build-firstframe.sh` at the real install names:

- libobjc: the NSObject root; `objc_alloc`, `objc_opt_new`, `objc_opt_class`
  and `objc_storeStrong` as guest code; every `retain_xN`/`release_xN` bridge
  for x0-x28 from one table; and reflection services (`class_respondsToSelector`,
  `class_getName` and the `.cxx_destruct` chain walk used by `-dealloc`).
- Foundation: NSString, `NSStringFromClass` and a one-object NSSet.
- UIKit: UIApplicationMain, UIApplication, UIResponder, UIScene/UIWindowScene,
  UISceneSession, UIWindow, UIView, UIViewController, UIColor, UITouch and
  UIEvent. A guest software compositor fills a BGRA8 framebuffer. The run loop
  pushes and pops a pool per iteration, hit-tests and dispatches `touchesBegan`.

The profile is fixed at 390x844 points and scale 1. It does not claim device parity.

## Runtime widening

The runtime widens only to what these images link:

- Hierarchy depth up to 8, with non-fragile ivar sliding. App subclasses compiled
  against ivar-free SDK headers are moved past the provider superclass's real size.
- Typed ivars, including struct encodings with field names.
- ARC RO flags, strong-ivar layout bytes and property lists.
- Protocols with inherited lists, optional and class methods, properties and
  relative (compact) description lists. These are metadata-only; types are
  bounded and never dispatched.
- Whole-chain method and Super2 lookup, with the root metaclass falling back to
  root instance methods.
- One 512-byte instance slot size (256 slots, still bump-allocated per run).
- Process exit: a guest `exit` discards live objects like Darwin does, but a
  dealloc callback or pool drain may not be in flight. Returning from main still
  requires complete ownership.

## Host services

The host services are SVC stubs exported by the UIKit provider; each origin is checked.

- `launch_info(buf, cap)` returns UISceneDelegateClassName. A bounded bplist
  reader parses it from the `--info-plist` file and follows only the
  scene-manifest path.
- `present(fb, w, h, stride)` requires 390x844 with stride 1560. It reads the
  guest range with a checked read and writes `frames/NNNN.rgba`, then
  `NNNN.sha256`. The host never creates or alters pixels.
- `wait_event(buf)` reads one strict JSON-lines event from `--input` and writes
  a fixed 32-byte record (kind, sequence, x, y, reserved). Accepted events are
  touch began/ended with finite in-bounds x and y, and quit. Anything else rejects.

```sh
bash scripts/build-firstframe.sh
cargo build --release
python3 scripts/firstframe-driver.py            # separate process: red, random tap, blue/green, quit
target/release/examples/firstframe_bench artifacts/firstframe 100   # cost; JIT parity on x86-64 Linux
```

The driver's negative controls:

- With no input, frame 2 never appears.
- A malformed event rejects.
- A forged framebuffer range (`UIKit-forged.dylib`) rejects.
- A missing `_UIApplicationMain` export (`UIKit-missing.dylib`) rejects.

Not supported: text, fonts, images, CoreGraphics/CoreAnimation, layers,
autolayout, multiple windows or scenes, gestures, blocks/GCD, threads and slot
reuse. Only one tap's worth of churn is designed for; slots run out after
roughly 240 allocations in a run.
