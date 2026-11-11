# Scalar superclass lookup and Super2

Sable supports a root class and direct subclasses with a raw eight-byte isa
header. [Validated Q payloads](payload.md) may extend the instance layout to 64
bytes, preserving the parent prefix. Ordinary scalar messages search the receiver's own method
table and then its parent's corresponding table. An explicit original
`objc_msgSendSuper2` bridge searches the supplied current class's superclass,
then tail-calls a checked guest ARM64 IMP. [Execution receipts](../evidence/super/RESULTS.md).
This is static method execution. Separate [fixed-layout instance services](instances.md)
now add bounded creation/disposal; NSObject initialization and UIKit remain absent.

## Graph and lookup

Registration collects every class/metaclass identity before validating edges.
Declaration and library input order do not determine inheritance order. Root
ordinary classes have no superclass; root metas point to themselves as isa and
the ordinary root as superclass. A child ordinary class points to its parent's
ordinary class. Its metaclass isa and superclass both point to the parent's
root metaclass. Registered identities must be unique and correctly typed. Cycles,
unknown/mixed-kind edges and depth greater than two reject. The existing cap is
64 total ordinary classes (128 ordinary/meta identities), not 64 roots plus children.

Observed RO flags are 2/3 for root ordinary/meta and 0/1 for child ordinary/meta.
The original header-only profile uses root start/size 0/8 and child 8/8.
Q payload profiles additionally validate contiguous local fields; child start
equals parent size. Metaclass fields remain 40/40. Existing name, cache, scalar type,
owning method/code, metadata cap and snapshot checks still apply. A superclass
may come from an explicitly supplied original dylib; a missing dependency fails.
No placeholder superclass is manufactured.

`__objc_superrefs` is a bounded, eight-byte regular pointer array. A slot can
name a registered ordinary class or metaclass. Both kinds occur in the original
compiler output. Superrefs and graph fields are immutable metadata snapshots,
not instructions. Prepared/live mutations and instruction aliases reject.

Ordinary messages retain the real receiver and canonical SEL when an inherited
IMP runs. Class messages use class-method tables; instance messages use instance
methods. The root-metaclass-to-ordinary-root fallthrough is outside this profile.
Existing nil ordinary messages still return scalar zero. Unknown nonnil selectors
fail explicitly. The C `class_conformsToProtocol` service still reports direct
adoption separately from method inheritance: the control checks an adopting
parent is true, its non-adopting child is false, and the child inherits methods.
This is distinct from inherited protocol definitions or NSObject's own methods.

## Super2 call boundary

The original bridge has exactly three guest instructions:

```asm
ldp x0, x17, [x0]    // checked {real self, current class-or-metaclass} pair
svc #0x5ab7         // private lookup returns guest IMP in x16
br x16              // guest tail transfer preserves the caller's LR
```

The SVC is accepted only at the registered original runtime origin. Exact stub
words, snapshots and cache are checked. The current ordinary/meta identity must
match the receiver's kind and class in this depth-two profile. Search begins at
its parent, never at the current override. Nil Super2, unsupported root-without-
super, malformed pairs, unknown current/receiver/SEL and missing methods reject.
The pair load is an ordinary checked guest instruction; no host dereference is
used for guest addresses.

The engine returns lookup results in x16, keeping real self in x0. x17 is scratch.
The bridge retains the modeled argument registers, platform/frame/stack/link state
and NZCV. Real guest IMP instructions and RET run; the child's continuation then
executes its additions or nested messages. Pair load, SVC, BR, method body and RET
all consume the shared instruction budget. Constructors use the same boundary;
no CPU reset or synthetic successful return is substituted.

## Legacy binding prerequisite

The cross-dylib legacy fixture made LLVM emit a large unsigned ULEB delta that
moves its 64-bit bind cursor backwards. Sable previously rejected that cursor
addition as overflow. Binding now uses the format's modulo-64 cursor arithmetic,
requiring every resulting cursor to stay inside the selected mapped segment.
Every actual eight-byte write still requires bounded non-executable storage.
Standalone ADD_ADDR, combined bind/add and repeated bind/skip forms have focused
regressions. Symbol addends, TLS-kind checks, operation caps and rebase arithmetic
retain their existing rules. This does not permit arbitrary pointer arithmetic.

## Reproduce and limits

```sh
bash scripts/check-super.sh
cargo test --release --test super
cargo test --release --lib objc::hierarchy_tests
python3 scripts/apple-oracle.py --super --output artifacts/super-native.json
```

The matrix includes absolute/compact methods, legacy/chained fixups, same-image/
dylib parents and constructor/plain execution. Instance override, class override,
inherited instance/class and nested calls return independently checked results.
A leaf assembly IMP captures self, SEL, ordered arguments/x8, frame, actual LR,
SP and incoming platform state. Separate Sable probes test additional modeled
scalar state and every call/return budget edge, including both Linux dispatch
modes. Third-level class metadata is explicitly rejected.

Deeper or dynamic hierarchies, non-Q/general layouts, general allocation/ARC, categories,
+load/+initialize, forwarding, general Protocol objects, inherited protocol
records, tagged/PAC isa, exceptions, FP/SIMD/aggregate ABI and guest threads remain
unsupported. Foundation/UIKit execution, UI recordings on Sable/Linux, concurrent
app capacity and any competitor app advantage remain unproved.

Primary format/ABI references are Apple's pinned
[ARM64 messenger](https://github.com/apple-oss-distributions/objc4/blob/fb265098298302243cd7eeaa1f63f0ba7786dd9a/runtime/Messengers.subproj/objc-msg-arm64.s),
[class records](https://github.com/apple-oss-distributions/objc4/blob/fb265098298302243cd7eeaa1f63f0ba7786dd9a/runtime/objc-runtime-new.h),
[direct conformance service](https://github.com/apple-oss-distributions/objc4/blob/fb265098298302243cd7eeaa1f63f0ba7786dd9a/runtime/objc-runtime-new.mm#L5490),
[bind cursor interpreter](https://github.com/apple-oss-distributions/dyld/blob/fd8d0c4d52320ebf64db34f3cb280310d905c5ae/mach_o/BindOpcodes.cpp),
and LLVM 21.1's [bind-delta encoder](https://github.com/llvm/llvm-project/blob/llvmorg-21.1.0/lld/MachO/SyntheticSections.cpp#L395).
Only format/calling facts were used; code, probes and runtime stubs are original MIT.
