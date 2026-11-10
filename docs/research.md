# Technical premise

Reviewed 2026-11-10 UTC. This project starts with execution rather than orchestration.
An API around existing Mac workers would not answer whether we can remove the
macOS guest's cost or improve cross-architecture execution.

## What the earlier investigation established

The precursor investigation on the development Mac inspected Xcode 27.2 and an
installed iOS 27.2 runtime. Its simulator runtime was ARM64-only; CoreSimulator
host components were ARM64/ARM64e. An original Foundation/Mach/Metal probe ran
through a temporary CoreSimulator device and failed as an ordinary subprocess
without the simulator environment. That experiment ran on macOS, not Linux.
The private inventory and runtime files have not been published in this repository.

The inspection found Darwin/Mach/XPC imports and host dependencies on IOSurface,
Metal and simulator-specific loading. An ARM64 Linux CPU alone supplies none of
those contracts. Apple's own Metal documentation describes simulator GPU work
using the host Mac's GPU. A simulator-targeted binary is a distinct Mach-O platform
from a device binary, even if both use ARM64.
[Apple Metal Simulator documentation](https://developer.apple.com/documentation/metal/developing-metal-apps-that-run-in-simulator),
[Apple Mach-O definitions](https://github.com/apple-oss-distributions/xnu/blob/f6217f891ac0bb64f3d375211650a4c1ff8ca1ea/EXTERNAL_HEADERS/mach-o/loader.h).

## The real comparison

Macbox here means **TUNA-NOPE/macbox**, not other similarly named remote-Mac
services. Its pinned README describes an x86 macOS guest on Linux/QEMU/KVM with
Reims vGPU, Metal-to-Vulkan translation, Xcode and an iOS Simulator pool. It
requires KVM, AVX2, Vulkan, at least 16 GB RAM and approximately 150 GB storage.
These are project-reported requirements. We have not reproduced its guest,
compatibility, performance or resource consumption.
[Macbox, commit 866b1f0](https://github.com/TUNA-NOPE/macbox/blob/866b1f0f9e918c2593c9f634785c9129d1ef6aae/README.md).

Reims is substantial GPU work, with its own guest/host graphics implementation.
Its progress is relevant to a full-guest comparison, but does not supply a direct
Darwin userspace layer or demonstrate arbitrary ARM64 iOS apps on x86 Linux.
[Reims, commit 1fe0acb](https://github.com/steelbrain/reims-vgpu/tree/1fe0acb68b287db36f1e6c7b9f45383940b8a76f).

Limrun's public guide describes Xcode and Simulator execution on remote Macs.
That is a working product category; a new translator must eventually match useful
app behavior, not merely report a faster cold process start.
[Limrun's iOS guide](https://docs.limrun.com/docs/guides/ios-without-a-mac).

## Chosen hypothesis and alternatives

**Hypothesis:** a Linux userspace runtime that implements the Darwin contracts
actually exercised by simulator apps can avoid full macOS guest boot/memory and
run guest basic blocks with limited translation overhead. Lower initialization
and per-instance memory could improve completed agent workflows per dollar.
This is an inference and a research direction, not an established speedup.

Sable starts with the loader, ARM64 execution and SVC boundary because they are
independently testable and needed before a direct runtime can run any code. The
current tests do not exercise the framework/service contracts that dominate the
feasibility question. Instruction translation alone is not the differentiator.

| Route | What it buys | Hard gate |
| --- | --- | --- |
| Full macOS guest | Reuse Darwin and framework behavior | Current ARM64 guest boot, graphics, memory and host constraints |
| Direct simulator compatibility (Sable) | Remove whole-guest overhead in principle | dyld, Mach, threading, Objective-C, frameworks and graphics correctness |
| Reimplement a selected UI/API subset | Control code and deploy on Linux | Limited app coverage; must report app-by-app compatibility |
| Remote Apple workers | Current simulator behavior immediately | Different cost/density problem; does not validate Linux execution |

Darling already implements large parts of Darwin/Mach compatibility, Mach-O
loading and framework work. It is a necessary architecture reference before
expanding our Mach implementation. touchHLE is a useful API reimplementation
precedent, with deliberately old/limited iOS targets. Neither is evidence of
current Sable compatibility. Any integration requires a component-level license
review rather than copying code into the MIT module.
[Darling source](https://github.com/darlinghq/darling),
[touchHLE source](https://github.com/touchHLE/touchHLE).

## Low-level contracts to prove

- **Executable loading:** chained fixups, shared caches, TLS, initial process
  layout, dyld interposition and simulator platform behavior.
- **CPU correctness:** flags, memory operations, NEON, atomics, ARM memory ordering,
  precise faults, code invalidation and the separate ARM64e/PAC problem. A direct
  native ARM64 backend would also need a controlled SVC/trap strategy.
- **Mach/process services:** port rights and ownership, message queues, complex
  descriptor transfer, VM operations, pthread work queues, timers, signals and XPC
  services. Stubbed successful calls will hide bugs; implement observable semantics.
- **Runtime/framework loading:** Objective-C dispatch, autorelease pools, Swift
  runtime behavior, Foundation and then one selected UIKit path. Trace missing
  contracts before increasing app scope.
- **Graphics:** shared surface lifetime, cross-process mappings, fences and
  synchronization before optimizing Metal/Vulkan submissions or claiming zero copy.

The published XNU definitions anchor binary layout, trap dispatch and Darwin call
numbers. ARM's ISA reference anchors instruction semantics. Sable's code is
original; these definitions are cited as specifications, not relicensed code.
[XNU syscalls](https://github.com/apple-oss-distributions/xnu/blob/f6217f891ac0bb64f3d375211650a4c1ff8ca1ea/bsd/kern/syscalls.master),
[XNU ARM64 exception dispatch](https://github.com/apple-oss-distributions/xnu/blob/f6217f891ac0bb64f3d375211650a4c1ff8ca1ea/osfmk/arm64/sleh.c),
[ARM A64 instruction reference](https://developer.arm.com/documentation/ddi0602/latest/).

## Technical feasibility and deployment rights

Our own MIT runtime does not grant permission to redistribute or deploy Apple's
SDKs, frameworks or operating-system images. Apple's current Xcode agreement
restricts Apple Software execution to Apple-branded products running macOS.
That is a separate constraint from CPU feasibility. This first milestone uses
only original assembly and general LLVM/Rust tools on AWS; no Apple runtime or
SDK is transferred there. Any later experiment with Apple-supplied components
needs the applicable rights resolved as part of its deployment design.
[Apple Xcode and SDKs agreement](https://www.apple.com/legal/sla/docs/xcode.pdf).
