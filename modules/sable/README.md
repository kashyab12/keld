# Sable

A small, inspectable starting point for modern Darwin compatibility research.

Sable accepts thin ARM64 Mach-O executables tagged `PLATFORM_IOSSIMULATOR` (7).
It implements a restricted ARM64 instruction set and runs it through either a
reference interpreter or an x86-64 native translator. A bounded dispatcher handles
Darwin `write`, `exit`, deterministic single-process `getpid`, and experimental
`mach_absolute_time`. Guest code cannot invoke arbitrary Linux syscalls.

The x86 backend translates instructions into native code, links direct branches,
and uses an RW-to-RX transition. Translation happens inside this crate. Guest
memory remains in checked segment buffers. There is no external emulation engine.

This is **not a complete iOS Simulator**. There is no dyld loading/binding,
Objective-C runtime, Foundation, UIKit, Swift runtime, Mach message transport,
thread scheduler, atomics, NEON, PAC, graphics or app-bundle support. Ordinary
compiler-generated programs will encounter unsupported instructions. ARM64e is
rejected. Run only the trusted research fixtures; Sable has not been audited as a
security boundary for hostile binaries.

```sh
# From the repository root. Works on Linux/macOS for the interpreter.
cargo run --release -- run modules/sable/fixtures/bin/hello.macho interp
# x86_64 Linux only:
cargo run --release -- run modules/sable/fixtures/bin/hello.macho jit
```

The expected stdout is `SABLE_ARM64_DARWIN_OK` followed by a newline. CLI diagnostics
on stderr report the guest instruction count, Darwin-call count and exit status.
Host exit 125 means a loader, instruction, memory or compatibility error.

`sable bench FILE 9` alternates the interpreter/translator order over nine pairs,
includes translation in every sample, and compares the complete CPU state, output,
exit status, syscall count and remaining instruction budget. The compute fixture
executes 30,000,006 instructions and sums integers 1 through 10,000,000 in `x9`.
The expected checksum is `50,000,005,000,000`. It measures arithmetic dispatch;
it cannot establish application compatibility, UI performance or cloud savings.

MIT licensed. [Design](../../docs/architecture.md), [next gates](../../docs/roadmap.md).
