# Static Objective-C protocols

Sable registers a bounded original protocol profile and implements
`objc_getProtocol`, `protocol_getName` and `class_conformsToProtocol` with real
identity/name/adoption data. These are scalar console contracts, not a
Foundation/UIKit runtime. [Execution receipts](../evidence/protocols/RESULTS.md)
separate native Apple reference from Sable execution.

## Records and ownership

Protocol definitions are exact 96-byte, eight-byte-aligned records in their
owning file-backed RW `__data` section. Disk isa and flags are zero. This first
profile accepts required-instance method descriptions only; inherited protocols,
optional/class descriptions, properties, Swift fields and other sizes reject.
The extended-type array must correspond exactly to the description type pointers.
Descriptions have 24-byte absolute entries with **null IMPs** and supported
unsigned 64-bit scalar signatures. They describe requirements and are never added
to executable dispatch tables. Their selectors join normal canonicalization.

`__objc_protolist` and `__objc_protorefs` are checked eight-byte slot arrays.
Observed protocol-list types are regular 0 (Apple linker) and coalesced 11 (LLVM);
references use coalesced 11. Final emitted records are checked independently by
`scripts/check-protocol-format.py`, including supported chained pointer formats.
A definition's body/name/descriptions must belong to that image. References and
adoption entries must name exact registered identities. Duplicate global names
are rejected; no weak-definition merging or dynamic protocol registration is
claimed.

Class and metaclass RO adoption pointers must agree. Lists have a bounded
pointer-sized count and exact registered protocol pointers; the compiler's
trailing null is not counted as an adoption. Conformance means a declaration,
not a check that required methods are implemented. A class with the same methods
but no adoption returns false; a declaring class without an implementation can
return true. Existing class methods still require valid owning executable IMPs.

Only validated protocol bodies become immutable snapshots. Ordinary `__data`
values are neither snapshotted wholesale nor charged as protocol metadata.
C-only images retain their existing data/string behavior. Data-section candidate
retention is bounded at 64 per image; excess ownership declarations are rejected
when a protocol body needs them. Definitions are capped at 64, direct adoptions
at 64 per class and descriptions at 128 per protocol. The existing aggregate
256 KiB metadata/string cap includes participating bodies. Body ranges cannot
alias one another, other retained metadata or code. Preparation also checks
metadata against all instruction ranges, preserving the alias correction.

## Services and state

The original explicit libobjc exports three separate `svc; ret` stubs:
`#0x5ab4` lookup, `#0x5ab5` name and `#0x5ab6` direct class conformance. These are
Sable-private traps, accepted only at their owning registered runtime origins.
Missing runtime exports, TLS aliases or wrong code reject. Services return only
in x0, keep the other modeled registers/flags/SP, and return through the guest
link. Every SVC/RET consumes the shared guest instruction budget, including in
constructors. They do not count as Darwin syscalls or reset the CPU.

Lookup accepts bounded printable ASCII names and returns a registered guest
pointer, or zero for an absent valid name. Name lookup returns the stable guest
name pointer for an exact registered protocol identity. The explicitly tested
`protocol_getName(NULL)` implementation extension returns the original guest
constant `"nil"`; this is not a general nullability guarantee from the public
API. Conformance returns canonical 0/1 from direct adoption for registered ordinary
classes/protocols. Either null operand returns 0; unknown nonzero identities
reject. Protocol records remain opaque: ordinary messages to them reject rather
than inventing Apple's Protocol class/isa.

Prepared/live mutation of bodies, names, references, descriptions, adoptions or
the nil-name constant rejects. Writable program values remain private per run;
registration precedes constructors and fresh runs start from pristine data.
Inheritance/super, inherited protocols, categories, ARC/allocation, dynamic
protocols, forwarding, caches, +load/+initialize, general signatures, exceptions,
FP/vector/aggregate ABI and guest threads remain unsupported.

## CPU prerequisite and checks

The optimized Bool caller emits TBZ/TBNZ. The interpreter and x86 JIT now test
the selected bit with ZR semantics, retain NZCV and apply the signed 14-bit
PC-relative displacement. The JIT uses native bit-test/conditional branches
with the normal target/budget guards. The independent Unicorn corpus covers
every bit position, both conditions/outcomes, ZR and backward/forward targets;
Rust checks also cover extreme signed displacements. No Bool declaration was
changed to avoid the instruction requirement.

Ubuntu Clang 18 also emits UBFIZ in that Bool caller. Sable implements the
unsigned bitfield family in both widths, with reserved width/N/immediate
combinations rejected. Inputs and outputs use ZR semantics; W operands truncate
before extraction and results retain NZCV. Direct x86 shifts/masks implement
both extraction and insertion forms. The independent corpus covers all 5,120
valid immediate pairs across both widths, ordinary/ZR/aliased registers and
eight input edges for the exact first-failing `0x531e7500` instruction. Signed
and merging bitfield families remain unsupported.

```sh
bash scripts/check-protocols.sh
cargo test --release --test protocols
cargo test --release --lib objc::protocol::tests
python3 scripts/apple-oracle.py --protocols --output artifacts/protocol-native.json
```

The original program has two protocols with the same method requirement, an
adopting class with a real IMP, an unrelated implementing class and a declaring
class without an IMP. It checks lookup/literal identity, names, positive/negative
adoption, missing names, null edges and real method result 42. An original
assembly caller checks callee-saved/platform/frame/link state against native
Apple; Sable-specific probes check all modeled scalar registers/flags and every
budget edge through the new services. Both method/fixup formats and constructor
variants check private reset and neighboring mutable storage.

Format/behavior references are pinned Apple's
[protocol records](https://github.com/apple-oss-distributions/objc4/blob/fb265098298302243cd7eeaa1f63f0ba7786dd9a/runtime/objc-runtime-new.h),
[runtime declarations](https://github.com/apple-oss-distributions/objc4/blob/fb265098298302243cd7eeaa1f63f0ba7786dd9a/runtime/runtime.h),
[protocol services](https://github.com/apple-oss-distributions/objc4/blob/fb265098298302243cd7eeaa1f63f0ba7786dd9a/runtime/objc-runtime-new.mm),
and LLVM21.1 [test-bit branch encoding](https://github.com/llvm/llvm-project/blob/llvmorg-21.1.0/llvm/lib/Target/AArch64/AArch64InstrFormats.td).
[Arm unsigned-bitfield semantics](https://www.scs.stanford.edu/~zyedidia/arm64/ubfm.html)
define the two extraction/insertion cases and reserved encodings.
Implementation and fixtures are original MIT code; no Apple implementation is
embedded, hosted or transferred.
