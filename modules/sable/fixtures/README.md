# Original fixtures

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
