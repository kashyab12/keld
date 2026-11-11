# Sable

A small, inspectable starting point for modern Darwin compatibility research.

Sable accepts thin ARM64 Mach-O executables tagged `PLATFORM_IOSSIMULATOR` (7).
It implements a restricted ARM64 instruction set and runs it through either a
reference interpreter or an x86-64 native translator. A bounded dispatcher handles
Darwin `write`, `exit`, deterministic single-process `getpid`, and experimental
`mach_absolute_time`. Guest code cannot invoke arbitrary Linux syscalls.

`Image::link` now loads explicitly supplied original dylibs, resolves supported
export tries and two-level imports, rebases data pointers, and eagerly applies
supported regular/lazy bind streams. It never searches the host for a dependency.
It also supports the bounded [64-bit chained format](../../docs/chained-fixups.md)
and data-only dylibs, with independent native and AWS execution checks.
The new original dynamic fixture uses math/helper libraries plus a minimal
original LibSystem ABI library implementing `write`, `exit` and `getpid`.
No Apple runtime libraries are distributed or loaded on Linux.

The x86 backend translates instructions into native code, links direct branches,
and uses an RW-to-RX transition. Translation happens inside this crate. Guest
memory remains in checked segment buffers. There is no external emulation engine.

The supported scalar subset now includes NZCV flags, conditional branches,
shifted arithmetic/logical operations, byte/halfword/word/doubleword memory access,
register-offset addressing, pre/post-index writeback and paired stack operations.
Each run owns its writable data and a 1 MiB stack. The native backend checks memory
bounds and permissions before loads/stores. Pair operations currently use the Rust
transition. Two frequently used guest registers stay in host registers; block
budgeting retains exact exhaustion behavior through a partial-block fallback.

An original Clang-compiled C fixture mutates an array through nested function calls,
spills registers to its stack and emits a 64-bit checksum. The same binary was
checked with Sable and Apple's installed Simulator. Independent Unicorn tests
compare all general registers, SP, PC, flags, instruction counts and tested memory.
[Validation and process-level QEMU benchmark](../../evidence/s1/RESULTS.md).

This is **not a complete iOS Simulator**. The restricted eager linker does not
execute Apple's dyld or implement shared caches,
general/dynamic TLS, weak/re-export lookup or general dynamic-loader APIs.
Bounded [plain-C TLS](../../docs/tls.md) and an original
[static scalar Objective-C2 runtime](../../docs/objc.md) now execute the documented
console profiles. There is no Foundation, UIKit, Swift runtime, Mach message transport,
thread scheduler, atomics, NEON, PAC, graphics or app-bundle support. Ordinary
compiler-generated programs will encounter unsupported instructions. ARM64e is
rejected. Run only the trusted research fixtures; Sable has not been audited as a
security boundary for hostile binaries.

Supported C initializer arrays execute before main in dependency order, once per
image per fresh run, with checked targets and a shared instruction budget.
[Startup ABI, evidence and limits](../../docs/initializers.md).

```sh
# From the repository root. Works on Linux/macOS for the interpreter.
cargo run --release -- run modules/sable/fixtures/bin/hello.macho interp
# x86_64 Linux only:
cargo run --release -- run modules/sable/fixtures/bin/hello.macho jit
```

The expected stdout is `SABLE_ARM64_DARWIN_OK` followed by a newline. CLI diagnostics
on stderr report the guest instruction count, Darwin-call count and exit status.
Host exit 125 means a loader, instruction, memory or compatibility error.

```sh
cargo run --release -- run modules/sable/fixtures/bin/dynamic/dynamic-8.macho jit \
  --lib modules/sable/fixtures/bin/dynamic/libsable_math.dylib \
  --lib modules/sable/fixtures/bin/dynamic/libsable_helper.dylib \
  --lib modules/sable/fixtures/bin/dynamic/libsable_system.dylib > checksum.bin
```

`engine::Prepared` retains immutable translated code across runs. Every invocation
forks private writable image bytes and a fresh stack; immutable image bytes are
shared through `Arc`. Native indirect calls/returns validate the guest target and
jump through an immutable native-offset table. A diagnostic Rust-dispatch mode
provides the matched benchmark control. [Dynamic execution evidence](../../evidence/s2/RESULTS.md).

`sable bench FILE 9` alternates the interpreter/translator order over nine pairs,
includes translation in every sample, and compares the complete CPU state, output,
exit status, syscall count and remaining instruction budget. The compute fixture
executes 30,000,006 instructions and sums integers 1 through 10,000,000 in `x9`.
The expected checksum is `50,000,005,000,000`. It measures arithmetic dispatch;
it cannot establish application compatibility, UI performance or cloud savings.
For externally validated arithmetic, use `fixtures/bin/checksum.macho`; it emits
the checksum as eight little-endian bytes. `compute.macho` is retained unchanged
for the initial evidence and internal state checks.

MIT licensed. [Design](../../docs/architecture.md), [next gates](../../docs/roadmap.md).
