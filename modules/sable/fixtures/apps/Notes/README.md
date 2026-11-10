# Sable Notes app oracle

Original MIT UIKit/Objective-C source and Swift XCTest UI tests. This app is the
first shared correctness fixture for future app benchmarks. It currently runs
on Apple's native Simulator. Sable cannot run it yet.

From the repository root, on a Mac with full Xcode and an available iOS runtime:

```sh
cargo build --release
python3 scripts/apple-app-check.py --output artifacts/app-check-new
```

The output directory must be new. The script generates a small Xcode project
using Python's standard library, builds against the locally installed SDK,
attempts the ARM64 app executable in Sable, then runs three external XCTest cases
on a fresh simulator UUID. It does not change `xcode-select`, reuse an existing
device, open Simulator.app, or use an in-app replay API. Xcode's public destination
lookup uses the default device set, so the runner creates one uniquely named
device there and shuts down/deletes only that UUID in `finally`. The receipt
checks that its UUID is absent afterward. An interrupted controller leaves its
owned UUID in the receipt for recovery; never use `shutdown all` or `delete all`.

The tests exercise:

- Search, tap, keyboard text entry, saving, process termination/relaunch,
  persisted text and reset of that text through the UI.
- Scroll through 60 rows, select the last row and verify the detail destination.
- Create a Metal device, clear a drawable to red and then blue, copy a pixel back
  through a GPU blit, check completion and pixel values, present that drawable,
  and independently sample the canvas in a composed screenshot after each tap.

The Metal case requires real Metal command execution. It fails when there is no
device, drawable, completed submission or correct screen pixel. A clear/blit is
a deliberately small graphics smoke test: shaders, texture uploads, complex
effects, frame pacing, video and sustained animation are not covered.

The native runner exits successfully only when all three tests pass with no
skips and its device is deleted. `native_oracle_passed` and
`sable_app_support_proven` are separate receipt fields. A successful native
oracle never promotes a failed Sable app attempt to a pass.

Build products, SDKs, frameworks, test runners and full `.xcresult` bundles stay
under ignored `artifacts/` or outside the repository. Publish only original
source, curated receipts and selected screenshots/log excerpts. App binary
hashes identify a local build; this is not a reproducible binary-build claim.
Nothing from the Apple SDK/runtime is installed on or transferred to AWS by
this runner.
