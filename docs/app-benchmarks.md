# App correctness and graphics gates

Sable's AWS CPU/linker benchmarks are useful for those components. They provide
no evidence of UIKit rendering, working touch/text input, or a usable iOS
session without a GPU. App compatibility must pass before app performance earns
a score. The original [Notes app](../modules/sable/fixtures/apps/Notes) and its
external XCTest driver establish the first reference workflow.

## The GPU question

Macbox's pinned README lists a Mesa Vulkan GPU, a render node and KVM as
requirements. Reims translates its Metal work through the host's Vulkan driver.
An integrated GPU can meet that requirement; a discrete board is not inherently
required. These are the project's requirements, not a reproduction of a
particular user's failure.
[Macbox requirements, pinned commit](https://github.com/TUNA-NOPE/macbox/blob/866b1f0f9e918c2593c9f634785c9129d1ef6aae/README.md#what-you-need).

Apple describes Simulator Metal work using the host Mac GPU. A simulator without
a visible window can still use that GPU. Our native UIKit oracle uses the
installed simulator's host graphics and does not prove GPU-free execution.
[Apple Metal in Simulator](https://developer.apple.com/documentation/metal/developing-metal-apps-that-run-in-simulator).

Keep these conditions separate in every report:

| Condition | Required evidence |
| --- | --- |
| Headless | No interactive window required; completed offscreen frames still verified |
| No exposed GPU | Host PCI inventory, device nodes, provisioned accelerators, graphics API device type and no remote render worker |
| Software rendering | Exact driver/backend and version, CPU device identification, correct output and complete CPU/RAM cost |
| Hardware rendering | Adapter/driver/API versions, render-node access, GPU utilization and total VRAM alongside CPU/RAM |

A missing `vulkaninfo` or `nvidia-smi` executable proves only that the tool is
missing. An empty Linux node list on macOS proves nothing about Mac graphics.
`scripts/graphics-inventory.py` records exposure and tool availability without
installing drivers; every new AWS smoke run saves that inventory. Current Sable
has no graphics backend to exercise on either a software or hardware device.

## First app pass

Run identical original app source and the same externally driven workflow on
each supported runtime. Pin the app, architecture, runtime, viewport, pixel scale,
locale, driver and test driver. ARM64 and x86 builds must be distinguished. Do
not compare the currently installed iOS 27.2 native reference to a different iOS
26 Macbox build as a matched benchmark.

The first app gate is three tests, with all failures and skips retained:

1. Search, enter text with the keyboard, navigate, save, relaunch the app process,
   verify persistence, reset, then verify the old text is gone.
2. Scroll to a noninitial row and verify the selected detail screen.
3. Submit real Metal work, verify its readback, tap to change it, and verify both
   colors in the presented screen capture. A successful API return or an
   accessibility label alone is insufficient.

Screenshots supplement state assertions. Before comparing screenshots across
systems, pin appearance, font/runtime, orientation, safe area, scale and status
bar, and specify crop/tolerances before collecting samples. Today's pixel checks
use an interior solid-color canvas; the captures are not general golden-image
equivalence tests. Timing a screenshot request does not measure frame-ready
latency unless the screenshot contains the requested state.

Notes is a first-party test app, not broad app compatibility. Add an independent
open-source app before a general claim, then cover animations, lifecycle and
background/foreground, networking, concurrent sessions and reset/isolation.
Neither a native reference pass nor a safe unsupported-operation error counts
as a completed Sable workflow. Do not silently substitute a remote Mac or a
different app implementation for Sable's Linux execution path.

## Performance after the app gate

Maintain independent CPU-only and accelerated results. If the same workflow
cannot run in a condition, report unsupported and the failure stage; do not
invent an infinite speedup or count only the cases that ran. Report success
rate, failed attempts and retries with every throughput or cost figure.

Measure from session request through the verified final frame/state. Report
worker startup, app build/install, launch to first usable frame, input to visible
response, screenshot encode/transfer, persistence/reset and teardown separately
as well as end to end. Use at least 30 randomized complete paired workflows per
condition after smoke checks, with raw records and confidence intervals.

Sweep concurrency on the entire host, including idle agent waits and sustained
UI activity. Capture p50/p95 latency, successful workflows/second, total RSS and
host memory, CPU time, GPU utilization/VRAM, power or billed cost where available,
OOMs and long-run failures. Include the Mac/VM, renderer, encoder, orchestration,
storage, networking and accelerator costs. Score successful correct workflows
per total dollar at matched latency, not translated guest instructions/second.

The 2–4× goal remains gated on these complete app results. The existing
[S2 improvements](../evidence/s2/RESULTS.md) compare internal Sable paths on
freestanding C fixtures and cannot satisfy this gate.
