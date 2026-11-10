# First app interaction reference and AWS graphics inventory

Observed 2026-11-10. **Sable does not run this UIKit app.** The successful app
tests below ran on Apple's native Simulator and define behavior Sable must
eventually match. The AWS check exercised Sable CPU execution, not an app UI.

| Path | Observed result | What it establishes |
| --- | --- | --- |
| Native Apple Simulator, Xcode 27.2 / iOS 27.2 build 24B8117, ARM64 | All 3 external UI tests passed, 0 skips | Reference app behavior including actual input, persistence/reset, scrolling and Metal pixels |
| Sable interpreter, same app's ARM64 executable | Exit 125, `unsupported dynamic load command 0x80000034`; 0 interactions | Chained fixups are the first observed loader failure. There is no Sable app pass |
| AWS Ubuntu 24.04, c7i.large, x86-64 | Remote exit 0, 30 Rust tests passed, hello/compute execution passed | CPU/linker regression check plus host graphics inventory. No UIKit/graphics execution |

## Native behavior checked

The original [app and tests](../../modules/sable/fixtures/apps/Notes) build with
the installed SDK. XCTest runs in a separate process and drives the app through
taps, keyboard input, scrolling and lifecycle commands. There is no command
endpoint inside the app that replays the desired outcome.

- Search for Item 42, open it, type `Sable app check`, save, terminate/relaunch
  the process, reopen the item and assert exact persisted text. Reset through
  the UI, reopen and assert the old text is absent.
- Scroll through the list to Item 59, tap it and verify that detail destination.
- Create a Metal device and render red into a drawable. Blit a pixel to a shared
  buffer, wait for command completion and assert exact RGBA. Check the interior
  canvas pixel in a composed screen capture. Tap to render blue, then verify
  both GPU readback and composed screenshot again. The device reports
  `Apple iOS simulator GPU`.

This is a clear/present/readback smoke test. It does not test shaders, texture
uploads, advanced Metal features, frame pacing, video or sustained animation.
The Mac's native graphics path is in use; no GPU-free rendering claim follows.
There was no request to open Simulator.app, but a headless test still uses
graphics services.

The first run failed: the initial app omitted scene lifecycle support, and
UIKit terminated it in
`___UIApplicationEvaluateRuntimeIssueForNoSceneLifecycleAdoption_block_invoke`.
We added an explicit scene manifest and scene delegate, then ran all three
tests successfully on a fresh owned UUID. The first Sable attempt also
incorrectly supplied Xcode's universal executable; the runner now extracts its
ARM64 slice before observing the actual chained-fixup rejection. Both failures
are retained in the [initial receipt](native-initial-failed.json).

Three fresh-device runs passed all three cases with no skips. The final run also
checks the screen pixel with the readback buffer's lifetime explicitly scoped
to the drawing operation. The final controller requires runner exit 0, exactly
three tests and zero failures/skips. Direct boundary checks rejected a nonzero
runner exit even with three passing tests, incomplete summaries and
failed/skipped/extra tests.

The successful [native receipt](native-01.json), [XCTest summary](native-01-tests.json),
[second fresh-device receipt](native-02.json), [second XCTest summary](native-02-tests.json),
[final fresh-device receipt](native-03.json), [final XCTest summary](native-03-tests.json),
[attachment manifest](selected-captures.json) and [Metal readback](metal-device-and-pixel-readback.txt)
contain the reference results. Selected screenshots show
[persisted text after relaunch](note-persisted-after-process-relaunch.png),
[red presentation](metal-red-presented.png) and
[blue presentation](metal-blue-presented.png).
These are original app captures, not golden-image comparisons across runtimes.
App source and binary hashes identify the local build; native timings are not
a performance benchmark. No SDK, runtime/framework binary, test runner or full
xcresult bundle is published.

## AWS observations

The [new run](aws-cpu-01/run.json) used a disposable c7i.large in us-east-1,
2 vCPUs / 4096 MiB, Ubuntu image dated 2026-11-08, Xeon Platinum 8488C and Rust
1.93.1. The [AWS instance-type response](aws-c7i-type.json) has `GpuInfo: null`.
The actual [host inventory](aws-cpu-01/results/graphics-inventory.json) records:

- One PCI display-class function, vendor `0x1d0f`, device `0x1111`, no bound
  driver reported. This must not be misreported as an empty PCI graphics list.
- No exposed `/dev/dri/renderD*`, `/dev/nvidia[0-9]*` or `/dev/dxg` device node.
- `vulkaninfo` and `nvidia-smi` absent. No Vulkan device or software renderer
  capability was measured, and no Mesa fallback was installed or exercised.
- No DISPLAY/WAYLAND display environment and no software-renderer override.

Sable executed its original freestanding fixtures directly on Linux. It did
not start a Mac, remote renderer, UIKit process, UI driver or graphics backend.
The [raw stdout](aws-cpu-01/stdout.txt), [stderr](aws-cpu-01/stderr.txt) and
[source manifest](aws-cpu-01/runtime-source-sha256.json) retain what ran. The
arithmetic timing in that log is excluded from all app-performance conclusions.

The AWS controller verified [zero remaining instances, volumes, security groups
and EC2 keys](aws-cpu-01/cleanup.json). Each native receipt verifies deletion of
its own simulator UUID. Other simulator devices were not shut down or deleted.

## Remaining gate

Implement the required loader/runtime/services and graphics contracts until
this app passes under Sable on Linux. Then run the same workflow on a host with
no exposed accelerator and a separately identified accelerated host. Preserve
unsupported cases and failed attempts. Only after app correctness passes should
we collect end-to-end latency, host density and successful workflows per dollar
against pinned Macbox/Limrun baselines. The full protocol is in
[app and graphics gates](../../docs/app-benchmarks.md).
