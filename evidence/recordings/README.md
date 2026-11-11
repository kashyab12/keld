# App interaction recording

[Watch the native reference video](native-app-reference.mp4). Its caption stays
visible throughout: **Native Apple reference - NOT Sable on Linux**.

The video shows the original Notes app's Metal red/blue toggle, scrolling,
search, text entry, save, process relaunch and reset. XCTest drives the app
externally. All three cases passed with zero skips on a newly created iPhone 17
simulator using Xcode 27.2 / iOS 27.2. Its UUID was deleted afterward.
The [receipt](native-reference-receipt.json) and [test summary](native-test-summary.json)
record the build/source hashes and cleanup. Sable's attempt at that same ARM64
app exits 125 at the unsupplied UIKit dependency, completing zero interactions.

This is an actual recording of Apple's native Simulator using the Mac graphics
stack. It is not a Sable UI, a Linux simulator pass, a GPU-free result or a speed
benchmark. The [manifest](manifest.json) identifies the original recording and
the published excerpt: trim from 50 seconds, scale to 480 pixels, retain normal
playback speed, encode H.264 and add the caption bar. No app frames are generated.
The published clip is about 73 seconds; startup/installation is omitted. The
full original recording, xcresult, built app and SDK content stay outside Git.

Reproduce on a Mac with the installed Xcode/runtime:

```sh
cargo build --release
python3 scripts/apple-app-check.py --output artifacts/app-recording-new --record
```

The runner targets only its created UUID, records through `simctl io
recordVideo`, sends SIGINT to finalize the movie, and removes that device in its
cleanup path. It does not open Simulator.app or alter an existing device.
