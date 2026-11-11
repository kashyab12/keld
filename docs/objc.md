# Static scalar Objective-C2

Sable registers a bounded original Objective-C root-class profile and dispatches
messages to the compiled ARM64 method bodies. This is an original MIT
implementation, with an explicit original libobjc fixture. It supplies no Apple
runtime, Foundation, UIKit, allocator or UI. [Execution receipts](../evidence/objc/RESULTS.md)
separate Sable on Linux from the native Apple reference.

## Registration and ownership

After eager legacy/chained fixups, the linker checks ObjC metadata in each
explicitly loaded image, canonicalizes selectors and publishes an immutable
lookup plan before C constructors execute. Each ObjC image must declare the
explicit `/usr/lib/libobjc.A.dylib` dependency. Missing imports remain errors.
The original library exports `objc_getClass`, `objc_msgSend` and a zero data word
named `_objc_empty_cache`; no Apple cache algorithm is implemented.

The initial profile accepts imageinfo version 0/flags 0x40, 40-byte root class
and metaclass records, 72-byte class RO records, and either 24-byte absolute or 12-byte on-disk relative method entries. A root class has no superclass; its metaclass points to itself and has
the ordinary class as superclass. Names must agree, cache/vtable fields must
match the supplied profile, and methods must name aligned code in their owning
image. Instances have one raw eight-byte isa word. An optional single explicit
isa ivar must have offset 0, type `#`, alignment 8 and size 8.

Supported class/instance method signatures return Q or @ with zero through
six Q/@ arguments, using the same 64-bit scalar register channel. Protocol
descriptions retain the unsigned-Q-only profile. Encodings exactly match the selector's
colon count and argument positions. Classrefs name registered ordinary classes.
Selectors are interned by byte name into deterministic guest SEL addresses;
method name pointers and selref slots are patched together, including duplicate
selectors across images. Names/encodings use bounded printable ASCII strings. Ranges, alignment, file
correspondence and arithmetic are checked. Limits are 64 total ordinary classes,
including supported children, 1,024 selectors/methods,
128 methods per list, 255-byte strings and 256 KiB aggregate registered metadata
(including the participating images' C-string sections). Plain C images
without ObjC metadata retain the existing image limit; the ObjC cap is not
applied to their string sections.

The compiler profile pins `-fno-objc-arc`, `-fno-objc-exceptions`,
`-fno-objc-relative-method-lists`. Paired links request either
`-no_objc_relative_method_lists` or `-objc_relative_method_lists`; the final
metadata, rather than frontend flags, determines the format.
The build probes the frontend switch: older Clang emits absolute records by
default and lacks that option. Final metadata is checked rather than trusting
flags or a presumed default. An explicit linker switch is always required. LLVM LLD places class
names/type encodings in `__cstring`; the loader retains that original owning
section as well as dedicated ObjC string sections. Selector stubs are ordinary
immutable checked guest code. Unsupported ObjC sections reject explicitly.

## Compact on-disk method records

The checked compact profile uses header `0x8000000c` in the owning image's
`__objc_methlist` section. That section is regular file-backed RX metadata,
4-byte aligned, included in the metadata cap/snapshot, and excluded from guest
instruction ranges. Executable-section flags are rejected. After all sections
are collected, every retained ObjC metadata range must also be disjoint from
**every** declared instruction range, regardless of section name/order. The
same checked-overlap rule is applied to metadata snapshots at preparation,
including callers that modify public `Image` ranges. This prevents a separate
code-section declaration from aliasing otherwise non-code metadata. A list header plus
`count × 12` bytes must fit entirely inside it; lists may be aligned to four
bytes without eight-byte alignment. Absolute lists still require header 24 and
owning `__objc_const` storage with eight-byte alignment.

Each compact entry has three signed 32-bit displacements. Each uses its own
field address after image placement, with checked signed addition and no extra
slide. The name displacement must target an exact eight-byte slot in the same
image's `__objc_selrefs`; dereferencing that slot gives an owning method-name
string. The other displacements target the owning type string and aligned
owning code IMP. Existing scalar encoding/selector/class rules still apply.
Direct-selector/shared-cache flags, signed/PAC lists and other strides reject.

Canonicalization patches the selector slot, never a compact relative field.
Compact bytes remain identical to the file after both fixups and execution.
The same dispatch plan accepts mixed absolute/compact images; no new service,
CPU instruction or fake runtime initialization was added. An aligned alternate
address inside owning code is permitted metadata: the loader does not infer
source-level function boundaries. Behavioral assertions must still verify that
the intended method ran and returned the correct value.

The [compact execution receipts](../evidence/objc-compact/RESULTS.md) retain the
paired formats, field targets, native reference and cross-architecture tests.

## Message boundary

`objc_getClass` uses private `svc #0x5ab2; ret`, bounds the guest name string and
returns a registered guest class address, or zero for an absent valid name.
The supplied messenger is:

```asm
objc_msgSend:
    cbz x0, nil
    svc #0x5ab3
    br x16
nil:
    mov x0, #0
    ret
```

The lookup trap is accepted only at the registered runtime instruction. It
checks a canonical selector and either a registered class receiver or an
aligned readable object whose raw isa names a registered class. Arena instance
receivers also use [owned lifetime checks](instances.md); retired, interior or
forged identities reject. A class message
uses the metaclass method table. The trap returns the checked guest IMP in x16;
self, _cmd, arguments, LR, SP, x18 and modeled flags survive the lookup. The
guest tail branch leaves LR intact, so the actual IMP returns to the original
caller. Nested messages use ordinary guest frames and links. No host pointer,
host method, synthetic successful result or CPU reset substitutes for the IMP.
Nil returns zero for this scalar profile. Unknown nonnil messages fail.

Metadata snapshots and the empty-cache word are compared at each class/IMP
lookup trap;
mutation fails rather than using a stale method plan. This is deliberately
bounded, uncached validation, not a tuned production messenger. The interpreter
and both Linux x86 JIT indirect-dispatch modes share this boundary. Traps,
branches and IMP instructions all consume the same run budget; private runtime
traps do not count as Darwin syscalls. Writable object state is private per run.
Conditional select instructions CSEL/CSINC/CSINV/CSNEG use the checked scalar
instruction fallback in the JIT and have independent full-state Unicorn checks.

## Reproduction and limits

```sh
bash scripts/check-objc.sh
cargo test --release --test objc
# Local native Apple reference; requires installed Xcode/iOS runtime:
python3 scripts/apple-oracle.py --objc --output artifacts/objc-native.json
```

The two original classes exercise real instance/class method bodies, nested
calls, cross-image selector equality, constructor-time access, nil and unknown
class lookup, unknown-selector rejection, repeated prepared runs and an
assembly caller/IMP checking ordered x2–x7 arguments, x8, self, _cmd, x18,
frame pointer, SP and the actual return link. x16/x17 are scratch; x9–x15 are
caller-clobbered and are not a native ABI promise. Registration tests reject
malformed class graphs, RO layouts, ivars, strings, method encodings, relative
lists, foreign IMPs, unsupported sections and missing dependencies. Runtime
tests cover metadata mutation, foreign traps, bad receivers/selectors and every
instruction-budget edge in a lookup/tail-call/return sequence.

Bounded [static protocol registration and direct introspection](protocols.md)
now works separately from method dispatch. Protocol method descriptions never
become executable IMPs; declaring conformance does not prove implementation.

[Bounded depth-two superclass lookup and Super2 calls](super.md) now support
static children with no added instance storage, including supplied dylib parents.
[Fixed-layout creation/disposal](instances.md) now supplies bounded runtime-owned
objects and object-pointer method transport without automatic init/ARC.

Deeper/dynamic inheritance, categories, inherited/dynamic/optional protocol forms,
+load/+initialize, ARC/retain-release,
general allocation/lifecycle, forwarding, dynamic class changes, caches, tagged/nonpointer isa,
shared-cache direct/signed method lists, PAC, exceptions, vector/FP/aggregate returns,
pthreads and destructors remain unsupported. These console checks do not prove
Foundation/UIKit startup, GPU-free app interactions or any competitor advantage.

Format and behavior references: Apple's pinned
[ObjC2 records](https://github.com/apple-oss-distributions/objc4/blob/fb265098298302243cd7eeaa1f63f0ba7786dd9a/runtime/objc-runtime-new.h)
and [ARM64 messenger](https://github.com/apple-oss-distributions/objc4/blob/fb265098298302243cd7eeaa1f63f0ba7786dd9a/runtime/Messengers.subproj/objc-msg-arm64.s),
and LLVM 21.1's [conditional-select encoding](https://github.com/llvm/llvm-project/blob/llvmorg-21.1.0/llvm/lib/Target/AArch64/AArch64InstrFormats.td).
Only published format/ABI facts were used; implementation and fixtures are original.
