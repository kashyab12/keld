# Milestones and measurements

The goal is a useful mobile sandbox with 2–4× more completed workflows per dollar
than a pinned competitor baseline. A compiler microbenchmark is an intermediate
measurement. We do not infer product economics from it.

## Sable execution gates

| Gate | Required evidence | Current status |
| --- | --- | --- |
| S0: first ARM64 execution on Linux | Original simulator-targeted Mach-O, checked loading, output/exit, instruction budget, x86 translation and raw cloud evidence | Implemented; see AWS evidence |
| S1: independently validated CPU semantics | Native ARM64 or independent emulator comparison, flags, loads/stores, calls/stack, precise faults, atomics/NEON test corpus | Scalar subset implemented and independently tested; atomics, NEON, full fault/ISA coverage remain open |
| S2: first dynamic Darwin process | A small first-party C/Objective-C program with dynamic symbols, TLS, memory mapping and required trap behavior | First-party dynamic C, eager/chained fixups, checked initialization, scalar TLS, static scalar ObjC2 root classes and direct protocol introspection implemented; VM calls and broader runtime startup remain open |
| S3: first framework path | Original Foundation program with real Mach port allocation/message semantics, multiple threads and teardown | Not implemented |
| S4: first visible app | Original UIKit app, deterministic screenshot, tap/text flow, redraw and reset, no fake successful services | Original native reference app/tests pass; Sable execution and general screenshot equivalence not implemented |
| S5: simulator service scope | Process/service inventory, framework/runtime availability, multiple concurrent app sessions and isolation checks | Not implemented |
| S6: competitive benchmark | Same app/task/version and correctness checks on Sable, native Mac and pinned Macbox; matched cost and host accounting | Not implemented |

The next work is the remaining S1 coverage and the missing ObjC/object-lifecycle
and Darwin service contracts needed by actual framework startup. Do not add
orchestration APIs while framework execution is absent. Do not expand the ISA randomly: use independently
checked semantics and actual traces to prioritize required operations.

The [native Foundation investigation](foundation-contracts.md) now identifies
checked image initialization and plain C TLV storage as bounded next contracts.
[Initialization](initializers.md) is implemented and independently exercised;
[Scalar TLV resolution/storage](tls.md) and bounded [ObjC2 root-class dispatch](objc.md)
and [static protocol introspection](protocols.md) are also implemented. Bounded
[depth-two superclass lookup and Super2](super.md) adds static inheritance and
guest superclass calls. [Fixed-layout instances](instances.md) add real bounded
creation/disposal, pointer methods and lifetime checks. [Q payload fields](payload.md)
add validated inherited storage, real setters/getters and exact extra-byte bounds.
An [original constant-string framework read path](strings.md) now executes
non-ARC literals through a guest getter and libc. The following
[ordinary ownership slice](ownership.md) adds bounded ARC counts, nested pools,
ordinary return-claim fallback and shared-budget guest deallocation. The retained
SDK-linked Foundation console executable now completes locally with original
providers, including actual ASCII uppercase and pool cleanup;
[the receipt](../evidence/ownership/retained-sdk-minimal.json) records the unchanged
input hash. The earlier missing-pool receipt remains historical. Full guest
pthread behavior, general object lifecycle, Unicode, optimized ARC handoff and
complete framework execution remain open. This console slice does not satisfy
S3's Mach-message and multithread requirements.

The [Notes app reference](../modules/sable/fixtures/apps/Notes) now exercises
external taps/typing, scroll/navigation, persisted state/reset and real Metal
readback plus presented pixels on native Simulator. Sable parses its ARM64
executable's sections, then rejects the missing UIKit dependency and completes zero app
interactions ([current attempt](../evidence/ownership/app-still-unsupported.json)).
UIKit itself remains unsupplied.
Restricted chained pointer linking now works on original LLVM fixtures, with
matching Apple-native outputs; [format coverage](chained-fixups.md) and
[execution evidence](../evidence/chained/RESULTS.md) describe the limits. A new AWS
CPU-only run records actual graphics exposure; it is not a GPU-free app pass.
See [observations](../evidence/app-readiness/RESULTS.md) and the
[required app/GPU benchmark protocol](app-benchmarks.md).

The S2 hill climb added checked native indirect dispatch and reusable translated
code with private writable state. On the measured dynamic fixtures, prepared
indirect dispatch improved 2.19–2.25× on the long call-heavy workload; preparation
reuse improved about 2.77× for short runs. These compare Sable execution paths
inside an existing worker. [Methods, costs and limits](../evidence/s2/RESULTS.md).

## The 2–4× test

Define the primary score as **successful, correct workflows / total dollar**.
Record completed workflows/second, p50/p95 end-to-end latency, and total host RAM
at each concurrency. Include host build/setup time when it is paid per workload,
idle model-wait gaps, storage, GPU, streaming CPU, networking and failed retries.
Compare x86 and ARM Linux paths separately. A cheaper host with slower tasks is
not automatically an efficiency win.

The same first-party apps must run on all systems. A workflow checks search,
text input, navigation, persisted state, screenshot readiness and reset. Include
real agent wait intervals and a continuous-UI stress case. Run a smoke gate first,
then at least 30 randomized paired task runs per condition, raw per-run records,
failure/censoring counts, confidence intervals and sustained host measurements.
Treat 2×/3×/4× as target thresholds; report ratios and uncertainty when measured.

Pin Macbox, QEMU, Reims, OS, Xcode, runtime, app builds, driver and host SKU.
Do not compare different iOS versions or supported app sets without stating that
the result is conditional. Today's S0 arithmetic benchmark cannot participate in
this comparison because Macbox runs a much larger stack and Sable cannot run its UI.

## Android and sandbox service, after the iOS execution work

The precursor Android experiments are useful warnings for the eventual monorepo
modules. They are not copied into this public source tree or revalidated here:

- Sharing a host lowered allocated device-hour cost while continuous UI work
  saturated CPU; concurrency alone did not improve throughput.
- Mapped warm templates reduced measured emulator PSS, but first-use latency
  worsened. PSS is not total host density or a bill reduction.
- Incremental checkpoint capture wrote less, but adding preparation/drain time
  made end-to-end completion slower in a corrected pair.
- Page-cache/prefetch work had incomplete controls; no predictor win was proven.

The Android module should target shared immutable memory, private state, workload-
aware prefetch, coordinated guest memory reclamation and incremental checkpoints.
Measure total host memory and complete checkpoint/restore time, not only a
favorable suboperation. Preserve failed attempts and sentinel/state checks.

The later service module needs session ownership, admission control, streaming,
idle pause/reclaim, crash recovery, billing and isolation. Its intended ELv2
licensing is separate from the MIT Sable runtime; no service code exists yet.
