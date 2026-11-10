# Chained linking executed on AWS and native Apple

Observed 2026-11-10. Sable now executes original LLVM-generated chained fixups
on Linux. The real UIKit app's first diagnostic has advanced from an unsupported
load command to an unsupplied UIKit framework. **No UIKit app interaction or
GPU-free app advantage has been demonstrated.**

| Program | Expected 8-byte integer | Apple Simulator | AWS interpreter | AWS native translator |
| --- | --- | --- | --- | --- |
| Chained C, 8 rounds | `000000038afd5bab` | Match | Match | Match |
| Chained C, 2,000 rounds | `efe1edd5865163fb` | Match | Match | Match |
| Chained C, 200,000 rounds | `a7151818301809c9` | Match | Match | Match |
| Imported data +8 / -16 and local pointer rebase | `0000000000000033` (51) | Match | Match | Match |

The programs write little-endian bytes. Hex above describes the integer value.
The three C workloads import functions through two original dylibs and a
minimal original Darwin ABI library. The additional assembly fixture imports
addresses with signed/unsigned addends and uses a local rebased pointer. Its
numbers dylib contains data with no executable instructions. This caught and
removed a loader assumption that every dylib needs nonempty `__text`.

## What ran

- The [native Apple receipt](apple-native.json) records all 11 original fixtures
  passing on the installed iOS 27.2 Simulator, build 24B8117. Four are the new
  chained programs; the others are regression controls. Apple's real installed
  LibSystem supplies the standard native ABI. Its private temporary device set
  was deleted and listed zero remaining devices.
- The [AWS run](aws-01/run.json) used Ubuntu 24.04 on a disposable c7i.large,
  x86-64 Xeon Platinum 8488C, Rust 1.93.1. Remote exit was 0. **34 Rust tests
  passed**, including chained full-outcome comparisons with the interpreter,
  both native indirect-dispatch modes, repeated private-state runs and reversed
  supplied-library order. The remote host also rebuilt the four chained
  programs with Linux Clang/LLVM and matched all eight interpreter/JIT outputs.
- [Raw AWS stdout](aws-01/stdout.txt), [stderr](aws-01/stderr.txt),
  [runtime source manifest](aws-01/runtime-source-sha256.json),
  [rebuilt binary hashes](aws-01/results/chained/rebuilt-sha256.json) and the
  [eight output files](aws-01/results/chained) preserve the Linux checks.
  The [checked-in fixture hashes](fixture-sha256.json) identify the original
  native-checked binaries. C rebuilds may produce different machine code;
  checksum equivalence is required rather than byte identity.
- Local quick/full recipe checks passed, including 28 Mac Rust tests,
  independent scalar ARM64 oracle checks and rebuilding/executing both legacy
  and chained fixtures. The new published recipe includes chained checks.

No Apple SDK, framework, runtime, test runner or Mac worker was sent to AWS.
The [graphics inventory](aws-01/results/graphics-inventory.json) records no
exposed render node or installed Vulkan tools. This CPU/linker run does not
exercise a graphics implementation. The controller verified
[zero remaining instances, volumes, security groups and EC2 keys](aws-01/cleanup.json).

## Format correctness and limits

LLVM generated pointer format 2 and import formats 1/2 for the new fixtures.
Synthetic tests additionally check pointer format 6, tagged high bytes,
format-3 signed 64-bit import addends, slides, malformed metadata/pointer slots
and 2,000 deterministic metadata mutations. Those additional formats have no
independent native fixture evidence yet. All patches are decoded/resolved
before changing bytes; no partial image is returned on link failure.

The implementation checks segment/table/page boundaries, file-backed pointer
slots, non-executable storage, non-overlapping strides, reserved bits, import
ordinals and bounded counts. ARM64e/PAC, weak imports, compressed symbol tables,
multiple starts per page and shared-cache linking remain unsupported. The
[format contract](../../docs/chained-fixups.md) links the pinned primary source.

## The real app still stops before startup

The [actual UIKit startup attempt](ui-startup.json) uses the same ARM64 executable
hash as the previously native-verified app. It now returns:

```text
required dylib not supplied: /System/Library/Frameworks/UIKit.framework/UIKit
```

Dependency resolution happens before its pointer patching. This result proves
the load commands are recognized; it does not prove the app's own fixups were
applied, Objective-C initialized, or UIKit rendered a frame. The supplied
libraries implement only the original fixture ABI. The next requirements
include runtime/framework behavior, Darwin services and
graphics/input contracts. The existing [real app/GPU correctness gate](../../docs/app-benchmarks.md)
still applies.

This change establishes a working linker contract, not a product-performance
win. The historical arithmetic timing also printed by the AWS smoke controller
is excluded from all app comparisons. There is no new 2–4× app result or
competitive Macbox/Limrun measurement in this report.
