# Keld

Mobile runtime research for running agent workloads on less hardware.

The first module is **[Sable](modules/sable)**: an MIT-licensed ARM64 Mach-O loader,
reference interpreter, and ARM64-to-x86-64 translator with a small Darwin call
dispatcher. The implementation lives here; it does not shell out to QEMU, a Mac,
or an existing emulator.

**Current stage:** original freestanding iOS Simulator-targeted assembly and
compiled C executables, including writable arrays, stack frames and nested calls.
Sable does not boot Apple's Simulator, load UIKit or run ordinary iOS apps yet.
There is no measured performance comparison against Macbox or Limrun.

```sh
cargo test --release
cargo run --release -- inspect modules/sable/fixtures/bin/hello.macho
cargo run --release -- run modules/sable/fixtures/bin/hello.macho
# Native ARM64 -> x86 translation, on x86_64 Linux:
cargo run --release -- run modules/sable/fixtures/bin/hello.macho jit
cargo run --release -- bench modules/sable/fixtures/bin/compute.macho 9
```

Requires Rust 1.93.1 or newer. The checked-in Mach-O files contain only our original
assembly and linker metadata. Neither Apple SDKs nor runtime libraries are needed
to build Sable or run these probes. See [fixture provenance](modules/sable/fixtures/README.md).

- [Technical premise and primary sources](docs/research.md)
- [Loader, translation and Darwin design](docs/architecture.md)
- [Milestones and benchmark gates](docs/roadmap.md)
- [Reproduce the AWS Linux check](docs/aws.md)
- [Observed Linux and native Apple execution](evidence/README.md)
- [Scalar CPU validation and QEMU benchmark](evidence/s1/RESULTS.md)
- [Licensing](LICENSING.md)

The goal is 2–4× better completed app workflows per dollar at matched correctness
and latency. That is a target to test. An arithmetic translator speedup is not
evidence that we have reached it.
