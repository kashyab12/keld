# Original constant-string framework slice

Sable executes a bounded non-ARC literal-read workflow using original Foundation
and CoreFoundation dylibs. A real guest NSString getter returns the stored guest
byte address; original guest strlen/strcmp and write check the contents. No host
string or method result replaces these instructions.
[Execution receipts](../evidence/strings/RESULTS.md).

The eight programs cover absolute/compact methods, legacy/chained pointers and
plain/constructor execution. They read distinct and equal-content literals from
two images, preserve each record's identity, check empty and 63-byte strings,
compare actual bytes and repeat from private fresh state. Equal content does not
merge object identities across images.

## Checked records and providers

`__cfstring` is retained as non-code metadata with checked file correspondence,
regular type, eight-byte alignment and exact 32-byte stride. At most 256 records
are accepted across participating images, within the existing aggregate metadata
cap. Validation runs after eager fixups and before guest constructors. Each row
contains an isa pointer, u32 flags `0x7c8`, zero u32 padding, a guest byte pointer
and u64 length. Length is 0..63, excludes NUL, and names a complete owning
`__cstring` range with nonzero ASCII bytes followed by NUL. Other formats,
UTF16, embedded NUL and larger strings reject.

Records and backing bytes must be read-only after fixups. Both become prepared
snapshots, excluded from instruction ranges. Preparation also checks permissions;
changing a public Image to make those ranges writable rejects. Registered
constant-class instance receivers must be exact registered record addresses.
A forged raw isa elsewhere cannot impersonate one, and the constant class cannot
be allocated through the bounded instance service.

The exact provided framework IDs are Foundation.framework/Foundation and
CoreFoundation.framework/CoreFoundation under /System/Library/Frameworks.
CoreFoundation exports `___CFConstantStringClassReference` as an alias to its own
registered ordinary class record. It is the class address, not a pointer cell.
That direct child has size/start 32/32 and no new ivars; its parent is the owning
Foundation NSString root with size32 and the guest UTF8String method. Existing
class, metaclass, IMP, cache, scalar signature and dependency checks apply.
Framework version metadata satisfies this declared slice only; it does not mean
compatibility with a complete Apple framework release.

A narrowly validated constant-only consumer may omit a direct libobjc load when
it has only cstring/imageinfo/constant sections, an explicit CoreFoundation
dependency and the validated loaded providers. Class/protocol/messenger providers
still require their explicit libobjc dependency. No missing image is invented.
The original literal helper exercises this real compiler output.

The sole new scalar pointer return encoding is ordinary no-argument
`r*16@0:8`, using x0 as a guest byte address. Protocol signatures stay Q-only;
general pointer/aggregate encodings remain unsupported. This supplies no host
pointer, implicit retention, ARC or method implementation.

## Reproduce

```sh
bash scripts/check-strings.sh
cargo test --release --test strings
cargo test --release --lib objc::constants::tests
python3 scripts/apple-oracle.py --strings --output artifacts/strings-native.json
python3 scripts/aws-smoke.py run --extended --output artifacts/strings-aws
```

The first-party programs use a genuine startup wrapper that calls their main
and guest exit. Native reference execution resolves Apple's installed framework
methods locally; Sable resolves our explicit original libraries. No Apple SDK,
framework or implementation is shipped or transferred to Linux.

## Following ownership gate

At the constant-only checkpoint, the retained unchanged-source ARC Foundation
executable resolved the supplied original framework dependencies, then rejected
its missing autorelease-pool import.
[Actual attempt](../evidence/strings/arc-minimal-still-unsupported.json).
That receipt predates the following ownership gate and remains unchanged.
The earlier native
[constant/lifetime and ownership observers](../evidence/strings/research-native-summary.json)
passed six references across macOS/iOS; their original sources are in
[the native-only reference directory](../modules/sable/fixtures/foundation/string-reference).
Failed compile/preservation expectations are retained in the curated summary.

The [following bounded ownership implementation](ownership.md) adds real strong
counts and nested pool draining, ordinary fallback return claims, specialized
register release and a shared-budget
guest dealloc continuation. Its original guest uppercase method allocates and
transforms actual ASCII input, and ARC keeps the UTF-8 backing alive through use.
The explicit `link_main`/`--main` entry policy handles a main return with no
initializers. The current constant probes continue to use their real wrapper.
Optimized return handoff, general Unicode/NSString, NSObject lifecycle,
ARC strong/weak fields, threads and UIKit remain outside this first read gate.
Sable still renders no app frames and completes no Notes interactions.

Format/ABI sources: LLVM21's
[constant emission](https://github.com/llvm/llvm-project/blob/llvmorg-21.1.0/clang/lib/CodeGen/CodeGenModule.cpp#L5950),
[record definition](https://github.com/llvm/llvm-project/blob/llvmorg-21.1.0/clang/lib/CodeGen/CGObjCMac.cpp#L1685)
and Clang's [interior-pointer ownership contract](https://clang.llvm.org/docs/AutomaticReferenceCounting.html#interior-pointers).
Original native metadata and observers establish the specific values described
above. All implementation, fixtures and local observer sources here are MIT.
