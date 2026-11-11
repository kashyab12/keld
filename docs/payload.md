# Bounded Q payload fields

Sable validates unsigned 64-bit instance fields in roots and direct children,
then executes original guest getters, void setters and superclass methods over
real allocated storage. Class RO size, not allocator rounding, determines the
requested extra-byte boundary. [Execution receipts](../evidence/payload/RESULTS.md).
No property/runtime shim replaces the guest loads, stores or returns.

## Layout contract

Ordinary instance sizes are eight-byte multiples from 8 through 64. Roots start
at zero, reserve the raw isa header and may declare its existing explicit
`isa`/#/offset0 record. Children start at their registered parent's validated RO
size and cannot overlap that inherited prefix. Root/direct-child graph rules,
meta 40/40 layout, cache checks and class/method/string caps remain unchanged.
All identities/layouts are collected before parent relationships are checked.

Ivar lists have exact stride 32 and at most eight rows per class. Every row names
an owning, complete, aligned four-byte cell in `__objc_ivar`, a bounded owning
name/type string, exponent 3 and size 8. Cells are globally unique and local
names are unique; children cannot redeclare inherited names. Beyond the optional
root isa, only Q fields are accepted. Payload ranges are eight-byte aligned,
contiguous and cover the local RO span exactly. Holes, unexplained size changes,
negative/high offsets, overlapping fields, foreign/interior/duplicate cells,
wrong type/size/alignment and inherited-prefix mismatches reject.

RO records, lists, cells and strings remain in prepared/live metadata snapshots
and are excluded from instruction ranges. The runtime does not slide ivars or
resize a superclass. Snapshot checks apply at preparation and private runtime
service boundaries; raw guest data access retains the existing mapped-region
permissions and is not per-object memory protection.

## Class size and allocation size

Validated class size is passed to instance creation. The ledger's requested span
is `class size + extra bytes`. Extra bytes are capped at 64 and the span must fit
the slot (512 bytes since the [FirstFrame profile](firstframe.md), which also adds
typed ivars and non-fragile sliding). Allocation still zeroes the entire slot and writes
raw isa, with the same private arena, stable backing pointers, lifetime checks,
disposal, quarantine and reset behavior. No mapped growth is introduced.

Native allocator minimum size, fast-path rounding and usable padding are separate
from layout. For example, root RO size 8 gives a child's first Q field offset 8;
a standalone object's physical allocation minimum does not move it to 16. A
24-byte child places requested extras at 24, not 32 or the end of our slot.
Native probes inspect fields/requested extras only, not usable padding or freed
memory, and assume no particular address-reuse pattern.

Four independently compiled profiles exercise these boundaries:

| Root size | Child size | Local fields | Child +64 requested extras |
| ---: | ---: | --- | ---: |
| 16 | 24 | one root Q, one child Q | 88 bytes |
| 8 | 16 | header-only root, first child Q at 8 | 80 bytes |
| 64 | 64 | seven root Qs, no new child fields | 128 bytes |
| 16 | 64 | one root Q, six child Qs | 128 bytes |

## Void setters and offset loads

Ordinary methods now additionally accept void return with the same exact Q/@
argument/frame/offset grammar. The original setter encoding is `v24@0:8Q16`.
Only the return may be void; protocol descriptions remain Q-only. The actual
setter IMP's STR instructions and RET run. Callers ignore the unspecified return
register and check side effects. No zero result is synthesized for a setter.

The split-parent compiler output loads an ivar offset with LDRSW literal. This
previously failed before field access. The interpreter and x86 translator now
load four bytes, sign-extend to 64, discard XZR destinations while still checking
the memory access, preserve NZCV and use the signed PC-relative displacement.
The x86 fast path uses MOVSXD behind existing read/budget guards. Independent CPU
cases include both signs, XZR and forward/backward data, with precise fault and
budget regressions. Other signed load/addressing families and prefetch remain
unsupported.

## Reproduce and limits

```sh
bash scripts/check-payload.sh
cargo test --release --test payload
cargo test --release --lib objc::payload::tests
python3 scripts/apple-oracle.py --payload --output artifacts/payload-native.json
```

The matrix includes all four sizes, both method/fixup representations,
same-image/dylib parents and constructor/plain execution. Programs verify real
setter/getter/super-sum results, zero/isolation across two live objects, extras at
the exact class-size boundary, disposal/recreation and constructor-held payload
with TLV state. Scope remains Q-only payloads, no added runtime API or general
layout/allocator, ARC, automatic init/dealloc, C++/weak destructors, deeper graphs,
FP/vector/aggregate ABI, threading or UI. Foundation/UIKit and a competitive app
advantage remain unproved.

Primary records are Apple's pinned
[ivar and RO definitions](https://github.com/apple-oss-distributions/objc4/blob/fb265098298302243cd7eeaa1f63f0ba7786dd9a/runtime/objc-runtime-new.h),
[size helpers](https://github.com/apple-oss-distributions/objc4/blob/fb265098298302243cd7eeaa1f63f0ba7786dd9a/runtime/objc-runtime-new.h#L2884)
and [Arm signed-word literal semantics](https://www.scs.stanford.edu/~zyedidia/arm64/ldrsw_lit.html).
Implementation and fixtures are original MIT; no Apple code is transferred.
