# Fixed-layout guest instances

Sable implements original bounded `class_createInstance` and `object_dispose`
services for registered root/direct-child layouts. The original header-only
profile is eight bytes; [Q payload layouts](payload.md) extend validated class
sizes through 64 bytes. Allocations
contain real writable guest bytes with checked identity and lifetime; original
factory, pointer-returning, comparison and superclass method bodies execute.
[Execution receipts](../evidence/instances/RESULTS.md). This adds no ARC, automatic
init/dealloc, general malloc/free or UIKit service.

## Storage and lifetime

A registered ObjC process with the complete instance-runtime export pair receives a
32 KiB RW arena at `0x72000000`: 256 slots ×128 bytes, each aligned to 16 bytes.
Each validated class size includes its eight-byte raw isa header and supported
Q fields. At most 64 requested extra bytes begin at that exact class size,
independently of allocator rounding or slot padding.
The arena and its bounded allocation ledger are allocated once during run setup,
before native code obtains backing pointers. Service calls do not resize mapped
regions, clone arena buffers or grow the ledger's reserved capacity. Guest
results are virtual addresses in this arena, never host pointers.

Creation requires an exact registered ordinary class. A null class returns nil;
unknown/metaclass/protocol identities and extra-byte requests above 64 reject.
The entire slot is zeroed, its raw isa is written, and its ledger record becomes
live only after storage writes succeed. Repeated live allocations have distinct
stable addresses. After 256 total allocations, exhaustion returns nil without
changing storage or records. Size limits are explicit supported-profile limits,
not native malloc behavior or a measured production capacity.

Disposal accepts nil or an exact live runtime-owned object with its original
expected isa. It scrubs the slot, retires the identity and returns nil. Interior,
static/image/stack, forged/unallocated, changed-isa and already disposed identities
reject. Slots stay quarantined until run reset, so a stale pointer cannot regain
a live identity by restoring a plausible isa. Ordinary and Super2 messaging apply
the same arena ledger checks before method lookup. The existing explicit static
raw-isa profile remains available outside the arena; static objects cannot be
disposed through this service.

Every run has fresh arena bytes and records, including after a prior run's budget
failure. Constructors and main share that run's storage and TLS context. Layout
checks reject image overlap and enforce a 64 MiB total mapped-memory cap including
stack, TLS and this arena. Stack/TLS/object ranges are disjoint. Both engines use
the same layout and mutable service boundary; Linux tests verify backing pointers
remain unchanged through repeated allocation/disposal.

These slots are not separate sandboxes or per-object memory protection. Raw guest
access still follows mapped-region permissions. The isolation checks concern
separate requested storage and private runs, not cross-tenant or UI capacity.

## Exact scalar signatures and calls

Class/instance method metadata accepts Q, @ or void returns and up to six Q/@
arguments, with exact frame sizes and offsets. For example, `@24@0:8Q16` is the
original factory, `@16@0:8` returns self, and `Q24@0:8@16` compares an object
argument. All use the existing scalar GPR channel. An @ argument is a guest value;
it is not automatically dereferenced or retained. Actual guest IMP instructions
must produce the result. Ordinary nil object-returning messages return x0=0.
Protocol description records retain their earlier unsigned-Q-only profile.

Only ordinary method returns may be void; real setters execute guest stores
and callers ignore unspecified return registers. Void arguments remain invalid.
Class/SEL arguments, quoted/qualified object types, pointers, blocks,
structs, FP/SIMD/aggregate ABI and general Objective-C encodings remain unsupported.
The loader still rejects unsupported class/layout/lifecycle flags rather than
silently skipping constructors or destructors.

Two original explicit libobjc exports each contain a checked `svc; ret` stub:
`#0x5ab8` creation and `#0x5ab9` disposal. Both exports are required together and
must be regular owning code. Only their registered SVC origins are accepted.
Metadata/cache snapshots and exact stub words are checked. Each service changes
only x0, retaining other modeled scalar registers, LR/SP/x18 and NZCV. Actual
SVC and RET instructions consume the shared guest budget. These are private
runtime services, not Darwin syscalls.

## Reproduce

```sh
bash scripts/check-instances.sh
cargo test --release --test instances
cargo test --release --lib objc::instances::tests
python3 scripts/apple-oracle.py --instances --output artifacts/instance-native.json
```

The original program uses an inherited guest factory and pointer methods, real
super calls, requested extra-byte zeroing/isolation, disposal/recreation, null
edges, a native scalar ABI caller and constructor-held object state. It also
accesses TLV storage while heap objects exist. Both method/fixup representations,
same-image/dylib parents, constructors, private resets, exact budgets, arena
saturation and malformed/lifetime boundaries are checked. Native probes inspect
neither Apple's raw isa bits nor freed bytes and assume no native address reuse
pattern.

Primary contract references are Apple's pinned
[creation/disposal declarations](https://github.com/apple-oss-distributions/objc4/blob/fb265098298302243cd7eeaa1f63f0ba7786dd9a/runtime/runtime.h),
[creation/disposal behavior](https://github.com/apple-oss-distributions/objc4/blob/fb265098298302243cd7eeaa1f63f0ba7786dd9a/runtime/objc-runtime-new.mm),
and [zeroing allocation helper](https://github.com/apple-oss-distributions/objc4/blob/fb265098298302243cd7eeaa1f63f0ba7786dd9a/runtime/objc-malloc-instance.h).
Only format/ABI facts were used; implementation and fixtures are original MIT.
Foundation/UIKit, graphics, concurrent app capacity and competitor advantage
remain unproved.
