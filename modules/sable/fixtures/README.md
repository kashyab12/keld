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
