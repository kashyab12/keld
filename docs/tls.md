# Scalar Darwin thread-local variables

Sable supports a bounded plain C Mach-O TLV contract in explicitly linked
original images. The same original console fixtures run on Apple's iOS
Simulator and in Sable; [receipts and limits](../evidence/tls/RESULTS.md) identify
each path. TLS does not supply Foundation, UIKit or graphics. A separate bounded
[scalar Objective-C2 implementation](objc.md) now handles original root classes.

## Metadata and linking

The loader retains regular TLS data (section type 0x11), zero-fill data (0x12),
24-byte descriptor records (0x13) and descriptor-pointer arrays (0x14).
Sections require checked bounds, alignment and file correspondence; templates
are restricted to one contiguous initialized/zero-filled span per image.
Executable TLS storage, duplicate/overlapping layouts and TLS initialization
function arrays (0x15) are rejected. Descriptor tables are writable non-executable
data; pointer arrays may become read-only after fixups.

Export kind 1 is a TLS **descriptor address**, distinct from regular/absolute
exports. A TLS export must name a complete record in its owning image. Legacy
and chained imports preserve this distinction and reject nonzero TLS addends,
including inline chained addends. Descriptor-pointer slots must resolve to
registered records, never to a guessed variable address.

After all fixups, the linker snapshots each pristine template and registers
exact descriptor addresses, resolver addresses and byte offsets. The current
disk profile requires key 0 and an in-template offset. Each image must explicitly
import the supplied original LibSystem `__tlv_bootstrap`; unresolved imports
remain errors. No Apple framework, dyld implementation or SDK is embedded.

## Private storage and resolver

Each run reserves its TLS arena at guest address `0x71000000`, before the JIT
captures the memory layout. Aggregate TLS storage including alignment padding
is capped at 1 MiB, with at most 1,024 descriptors. Loaded image bytes plus the
arena must fit the existing 64 MiB loaded-memory cap. Overlap with image or
stack reservations is rejected. Writable buffers and initialization flags are
private to that run; immutable template bytes stay in the prepared image.

First access copies the owning image's pristine template into its fixed arena
slot. Repeated accesses return the same checked guest base plus offset. A new
run starts fresh, including when constructors changed TLS before main. Resolver
calls do not resize memory or reset the CPU. Storage is reserved up front;
this does not reproduce Apple's host-allocation timing.

The original guest bootstrap is `svc #0x5ab1; ret`. This is a **Sable-private
trap**, not a Darwin syscall number. It is accepted only at a registered
resolver and for an exact registered descriptor. Resolver/key/offset words
are compared with the immutable plan on each access; changed words reject the
run. X0 becomes a guest address, never a host pointer. Modeled scalar registers,
SP and flags are otherwise preserved while PC/instruction budget advance
normally. The stub's RET uses the ordinary checked guest link; it does not use
the initializer return sentinel. Private resolver traps are not counted as
Darwin syscalls.

The interpreter and Linux x86 translator share this checked boundary. The new
logical-immediate operations have native translation; conditional compare uses
the existing checked Rust instruction fallback. Both are covered by full-state
Unicorn differential cases, including mask rotation/replication, SP/ZR roles,
all conditions, widths and incoming NZCV combinations.

## Checks and scope

```sh
bash scripts/check-tls.sh
cargo test --release --test tls
```

The LLVM-only fixtures contain two original dylibs, with initial values 17/0
and 29/0 and final values 23/7 and 41/9. They check stable/distinct addresses,
scalar resolver ABI, constructor access and both legacy/chained fixups. The
assembly preserves native x18/frame baselines and compares the actual BLR link.
Prepared runs reset independently; Linux checks interpreter/JIT parity in both
indirect-dispatch modes. Synthetic tests cover call/trap/return budget edges,
unknown/mid-record descriptors, mutated words, foreign traps, caps and malformed
metadata. Native reference: `python3 scripts/apple-oracle.py --tls`.

This is one logical TLS context per run. Guest pthread lifecycle/scheduling,
vector ABI, C++ TLS construction/destruction, `_tlv_atexit`, shared-cache keys
and dynamic TLS loading remain unsupported. No multi-threaded Foundation or
UIKit pass follows from these console checks.

Format/ABI references: Apple's
[descriptor definitions](https://github.com/apple-oss-distributions/dyld/blob/fd8d0c4d52320ebf64db34f3cb280310d905c5ae/libdyld/ThreadLocalVariables.h)
and [TLV setup](https://github.com/apple-oss-distributions/dyld/blob/fd8d0c4d52320ebf64db34f3cb280310d905c5ae/libdyld/ThreadLocalVariables.cpp),
LLVM's [Darwin preserved set](https://github.com/llvm/llvm-project/blob/7b1b7b1af91070d07492cf942942d40a51ceebe2/llvm/lib/Target/AArch64/AArch64CallingConvention.td),
and the [A64 conditional-compare encoding](https://github.com/qemu/qemu/blob/v10.1.0/target/arm/tcg/a64.decode).
Sable's implementation and fixtures are original MIT code.
