# Dynamic linking and reusable execution

Measured 2026-11-10 on two new AWS `c7i.large` instances. Sable now executes an
original dynamically linked C program with transitive library imports, a rebased
private counter pointer and a minimal original LibSystem ABI implementation.
The same executables and math/helper dylibs produce matching checksums in Apple's
iOS 27.2 Simulator, where they use Apple's installed LibSystem.

Two scoped hill-climb results replicated:

- Checked native indirect dispatch improved **2.19–2.25×** over Sable's Rust
  dispatch path on the long, indirect-call-heavy workload.
- Reusing a prepared image/JIT improved **2.77–2.78×** for short fresh runs,
  excluding one-time preparation. That cost is reported below.

These are Sable worker-path comparisons. They do not establish a 2–4× advantage
over Limrun, full simulator support or an all-in service cost reduction.

## Compatibility and correctness

The executable imports `sable_mix` from its math dylib. Math imports `sable_bias`
from the helper dylib and rebases a private counter pointer. The original minimal
system dylib supplies `_write`, `_exit` and `_getpid` under the standard LibSystem
identity. The fixture uses write/exit; getpid remains Sable's deterministic
single-process identity. No Apple implementation is included in these dylibs.

All three variants passed [the corrected Apple oracle](apple-oracle-with-system.json):

| Mixing rounds | Expected checksum | Guest instructions in Sable |
| ---: | --- | ---: |
| 8 | `000000038afd5bab` | 172 |
| 2,000 | `efe1edd5865163fb` | 36,028 |
| 200,000 | `a7151818301809c9` | 3,600,029 |

The first [native attempt](apple-oracle-initial.json) omitted a LibSystem
dependency and failed with `libdyld.dylib not found`. Adding an explicit standard
ABI dependency fixed native startup. Apple resolves it to its own LibSystem;
Sable requires the original minimal library to be supplied explicitly. The two
environments do not execute identical LibSystem implementations. Both temporary
Apple device sets were emptied and their owned devices deleted.

Each final AWS instance passed **30 Rust tests**, including dependency identity
and version failures, export resolution, malformed bind streams, rejected
constructor/TLS metadata, writable-state reset, immutable sharing and indirect
target/budget validation. Independent Unicorn 2.1.4 checks passed **7,437 cases
per engine per instance** over three seeds. New cases cover literal loads,
indirect branches and BLR with X30 as both source and link register.

The tests compare full guest registers, PC/SP, flags, budget and tested memory.
Worker benchmarks also compare every complete outcome, including writable image
bytes and the full stack, against the interpreter after each run. Private counter
state resets for every invocation. Linux Clang rebuilds of all dynamic fixtures
also emitted the expected checksums.

The current fixtures exercise regular/non-lazy pointer binding. The parser's
selected lazy-stream opcodes are not a validated implementation of native lazy
binding or Apple's binder helper. Constructors, TLS, chained fixups, weak and
re-export resolution, shared caches and framework initialization remain unsupported.

## Protocol

Both instances used Ubuntu 24.04, Intel Xeon Platinum 8488C, Rust 1.93.1, the same
Sable/benchmark executable hashes and identical fixture bytes. Instances could
share underlying hardware; this is not a representative fleet sample.

Each workload ran 31 randomized complete blocks containing four variants:

1. Link objects, compile code, execute with Rust indirect dispatch.
2. Link objects, compile code, execute with native indirect dispatch.
3. Reuse linked/prepared code, execute with Rust indirect dispatch.
4. Reuse linked/prepared code, execute with native indirect dispatch.

Every variant allocates fresh private writable memory and stack. Immutable image
bytes share allocations when the prepared image is reused. Native dispatch checks
guest target ranges, alignment and budgets before jumping through a trusted
read-only offset table. Rust dispatch provides the matched control.

Object bytes are already in the worker's memory. These timings exclude file reads,
process spawning, scheduling, network transport and VM boot. One-time linking and
preparation are measured separately. All variants get two warmup runs before the
recorded blocks, which are pinned to CPU 0. Complete-state equality checks and
journal writes happen outside the timed execution interval.

The two runs contain **744 dynamic samples, zero state mismatches**. Journals
record each result before the next sample. Intervals below use 5,000 paired
bootstrap resamples of the blocks within each instance; they do not estimate
fleet-wide uncertainty. Short-run microsecond timings include allocator effects.

## Results

All medians are microseconds. Prepared Rust/native columns both include fresh
guest state allocation/reset and execution, while retaining the linked image/JIT.

| Run | Rounds | Cold Rust | Cold native | Prepared Rust | Prepared native |
| --- | ---: | ---: | ---: | ---: | ---: |
| 01 | 8 | 61.75 | 63.65 | 23.13 | 22.90 |
| 02 | 8 | 64.87 | 66.16 | 23.34 | 23.85 |
| 01 | 2,000 | 182.64 | 124.46 | 133.65 | 79.06 |
| 02 | 2,000 | 164.72 | 107.96 | 126.52 | 72.50 |
| 01 | 200,000 | 10,149.91 | 4,710.04 | 10,453.88 | 4,644.17 |
| 02 | 200,000 | 10,025.22 | 4,609.51 | 10,141.97 | 4,624.87 |

| Comparison | Run 01 ratio [paired 95% interval] | Run 02 ratio [paired 95% interval] |
| --- | --- | --- |
| Native dispatch, prepared long workload | 2.251× [2.212, 2.273] | 2.193× [2.168, 2.246] |
| Native dispatch, cold long workload | 2.155× [2.128, 2.185] | 2.175× [2.143, 2.216] |
| Preparation reuse, short native workload | 2.779× [2.723, 2.905] | 2.774× [2.660, 2.951] |
| Preparation reuse, long native workload | 1.014× [0.997, 1.025] | 0.997× [0.982, 1.020] |

Reuse adds little when execution dominates. Native dispatch itself gives no
reliable short-workload win and adds some cold compilation overhead. These
optimizations address different costs; they do not multiply into a universal gain.

Short-workload linking/preparation was observed at 109.41/105.52 microseconds.
Including that one-time cost across the 31 prepared samples gives average costs
of **27.36/26.95 microseconds per run**. These are single observed setup costs,
not a startup-latency distribution. Each prepared run shares 84,208 logical bytes
of immutable image data; this is not measured total host RAM savings or density.

Raw [run 01 summary](run-01/results/dynamic-summary.json),
[run 02 summary](run-02/results/dynamic-summary.json),
[run 01 samples](run-01/results/), [run 02 samples](run-02/results/).
The earlier scalar/QEMU process-level benchmark was rerun as a regression check
and is stored alongside these results. It has a different timing boundary from
this worker-level experiment and should not be combined with these ratios.

## Reproduction and remaining work

```sh
bash scripts/check-dynamic.sh
cargo test --release
python3 scripts/aws-smoke.py run --dynamic --output evidence/s2/new-run
```

AWS source/fixture manifests, raw stdout/stderr, sample journals and rebuilt
checksum bytes accompany both runs. Readback confirmed zero remaining instances,
volumes, security groups or public keys for [run 01](run-01/cleanup.json) and
[run 02](run-02/cleanup.json). Private recovery state stays outside Git.

The next compatibility gate is broader Darwin startup: TLS, initialization and VM
calls, then the Mach/Objective-C/Foundation contracts actually required by a probe.
No UIKit, graphical simulator, concurrency isolation or matched Limrun workflow
benchmark was executed. Product cost/performance leadership remains unproven.
