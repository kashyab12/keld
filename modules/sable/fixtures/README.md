# Original fixtures

`payload-main.m` is an original Q-field/void-setter probe. `build-payload.sh`
emits root/child sizes 16/24, 8/16, 64/64 and 16/64 with both method/fixup forms,
same-image/dylib parents and constructors. Sixty-four positive programs check
real field access, superclass sums, exact extra-byte boundaries, disposal/reset
and TLV coexistence. `check-payload-format.py` independently inspects final
ivar cells/layouts/signatures. Only original LLVM files are in `bin/payload`.

`instance-main.m` and `instance-runtime.s` are original headerless creation and
disposal probes. `build-instances.sh` uses the original TLS/System and scalar ABI
helpers, both method/fixup formats and same-image/dylib parents. The sixteen
positive programs exercise object-pointer results/arguments, zeroed, isolated extra
bytes, actual inherited/super methods and constructor/main/TLV coexistence. The
independent `check-instance-format.py` validates the emitted graph and exact Q/@
encodings. `bin/instances` contains only original LLVM outputs, no Apple code.

`super-main.m`, `super-runtime.s` and `super-abi.s` are original headerless
inheritance/Super2 probes. `scripts/build-super.sh` links both method-list and
fixup formats with same-image or supplied-dylib parents, plain/constructor
variants, an intentionally rejected third level, and a direct-adoption control
on a subclass with inherited methods. Only original LLVM
outputs are retained in `bin/super`; no Apple runtime is embedded. The independent
`check-super-format.py` validates emitted class/metaclass graphs and superref
identities, including legacy and chained import names.

`hello.s` and `compute.s` are original assembly, released under MIT. `bin/*.macho`
are their linked ARM64 executables, not Apple binaries. Each has an iOS Simulator
platform tag (7), an `LC_MAIN` entry, no imported symbols, no loaded dylibs,
and no rebasing or binding records. An `LC_LOAD_DYLINKER` string is present as
linker metadata; Sable enters the freestanding entry directly and does not load
dyld. Ad hoc code signatures are linker-generated, not Apple-issued identities.

Rebuild with Clang and Rust 1.93.1's bundled LLVM linker:

```sh
bash scripts/build-fixtures.sh
cmp artifacts/hello.macho modules/sable/fixtures/bin/hello.macho
cmp artifacts/compute.macho modules/sable/fixtures/bin/compute.macho
```

The assembler needs no SDK headers or libraries. On Linux install `clang`;
Rust's installed host toolchain supplies `rust-lld`. Set `SABLE_LD` for a different
linker path. The script uses `arm64-apple-ios17.0-simulator` and explicitly sets
the simulator platform at link time. This older minimum OS tag describes the
fixture, not compatibility with an Apple iOS runtime. Sable runs no Apple runtime.

These binaries are deliberately small and freestanding. Loading one is a CPU,
loader and call-dispatch milestone, not proof that SpringBoard or UIKit boots.

`workload.c` and `workload-start.s` add a Clang-compiled, freestanding C workload:
64-element writable array, 2,000 mixing rounds, nested calls, condition flags and
stack spills. Its eight output bytes encode `0x8e65b49356b423fa` little-endian.
`checksum.s` executes the original arithmetic loop and emits `50,000,005,000,000`
as eight little-endian bytes. Both outputs were checked in Apple's iOS 27.2
Simulator and Sable. Binary output should be captured to a file, not displayed
as terminal text.

The original `compute.s` stays unchanged for historical evidence; its empty output
cannot independently verify a QEMU checksum. Current external benchmarks use
`checksum.macho` instead. `scripts/macho-to-elf.py` repackages original segment
bytes at the same guest addresses and adapts only fixed syscall-protocol
instructions for QEMU's Linux userspace ABI. It is deliberately fixture-specific.

The checked-in C fixture was built with Apple Clang 21.0.0 and LLVM lld 21.1.8.
Rebuilds with a different Clang can produce different machine code. CI/AWS rebuild
the C fixture with Linux Clang and verify its emitted checksum; byte identity is
required for the assembly fixtures only.

The `bin/dynamic/` fixtures are built from `dynamic-main.c`, `dynamic-math.c`,
`dynamic-helper.c`, `dynamic-start.s` and `dynamic-system.s`:

- Main imports a function pointer from the math dylib.
- Math imports a helper function and rebases a private counter pointer.
- The original minimal system dylib implements three Darwin ABI functions.
- Every run emits a known 64-bit checksum after 8, 2,000 or 200,000 mixing rounds.

Rebuild with `bash scripts/build-dynamic.sh`; verify all rebuilt checksums with
`bash scripts/check-dynamic.sh`. The checked-in executables and math/helper dylibs
also run in Apple's iOS 27.2 Simulator. Native Apple execution uses its installed
LibSystem; Linux execution uses the original explicitly supplied minimal system
dylib. None of the checked-in dylibs is copied from Apple software.

`bin/chained/` contains the same original dynamic C workloads encoded with LLVM's
`-fixup_chains` option. `chained-addend.s` adds positive and negative imported
data-pointer addends and a local rebase. `chained-numbers.s` is its original
data-only dylib. Their expected output is the eight-byte integer 51. Rebuild
with `SABLE_FIXUPS=chained bash scripts/build-dynamic.sh`, or rebuild and check
all four programs with `bash scripts/check-chained.sh`. The Apple oracle's
`--chained` option independently executes their checked-in binaries in a
temporary private simulator set. Format coverage and results are in
[the chained-fixup report](../../../evidence/chained/RESULTS.md).

`bin/initializers/{offsets,pointers}` contains original C/assembly images for
initializer scheduling. A shared leaf has two ordered callbacks; parent and
sibling each depend on it. Main's two callbacks include assembly checks of
argc/argv/envp/apple/ProgramVars, then main verifies all data mutations. Rebuild
with `bash scripts/build-initializers.sh`; `bash scripts/check-initializers.sh`
checks both formats against `init-expected.txt`. The pointer format uses legacy
fixup streams and eager binding in Sable; `init-binder.s` traps if an unsupported
lazy binder is actually invoked. Apple-native checks resolve real LibSystem.
The four [Foundation research sources](foundation) are separate SDK-dependent
native probes, not passing Linux fixtures.

## Original Objective-C2 fixtures

`objc-root.m`, `objc-other.m`, `objc-abi.s`, `objc-runtime.s` and `objc-start.s`
are original headerless scalar probes and an original explicit libobjc bridge.
Build with `scripts/build-objc.sh`; execute with `scripts/check-objc.sh`. The
checked-in `bin/objc/{chained,legacy}` contains these original LLVM-linked
programs/libraries only. No Apple SDK, implementation or framework is included
or needed to execute them in Sable. The native Apple reference resolves the
absolute System/libobjc identities to Apple's installed local runtime; its
reference result is separate from Linux. See [the exact contract](../../../docs/objc.md).

`bin/objc-compact` is the same original source linked with 12-byte on-disk
relative method lists. The paired `bin/objc` control retains 24-byte absolute
lists. `check-objc-format.py` independently asserts the final emitted format,
field-relative targets and negative IMP displacements. All built fixtures run
through the same output/ABI/constructor tests; format acceptance is not counted
as execution. Compact RX bytes must remain unchanged after linking/execution.

## Static protocol fixtures

`protocol-main.m`, `protocol-abi.s` and `protocol-runtime.s` are original headerless
probes and explicit runtime stubs. `build-protocols.sh` emits both class method
formats and both fixup modes, with plain/constructor variants; checked-in files
are in `bin/protocols`. `check-protocol-format.py` independently inspects emitted
protocol bodies, null-IMP descriptions, extended types and class adoption before
execution. The same final binaries are checked on native Apple Simulator locally.
No Apple implementation or SDK is distributed.
