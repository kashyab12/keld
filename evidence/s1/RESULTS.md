# Scalar execution and QEMU comparison

Measured 2026-11-10. Sable now runs an original Clang-compiled C program with
writable arrays, stack frames, nested calls, shifted arithmetic and NZCV-based
branches. Its emitted checksum matches Apple's iOS 27.2 Simulator and QEMU.
The optimized x86 translator was **1.36–1.38× faster on arithmetic** and
**1.58–1.59× faster on the C workload** than QEMU userspace in two fresh EC2 runs.
These are small freestanding programs, not UIKit or a Macbox/Limrun comparison.

## What changed

- Added selected scalar loads/stores (1/2/4/8 bytes), unsigned and signed offsets,
  register extension/scaling, pre/post-index writeback and paired stack accesses.
- Added arithmetic NZCV flags, all condition codes and shifted arithmetic/logical
  operands. Explicitly reject unsupported/reserved and constrained-unpredictable forms.
- Added private writable memory and an aligned 1 MiB stack per run, with permission
  checks, precise failed-access state and whole-range validation for pair stores.
- Added native x86 scalar memory lowering. Pair accesses use the checked Rust path.
- Cached two statically selected guest registers in R12/R13, the budget in R14 and
  PC in R15. Common exits flush state and restore all four host registers.
- Charged arithmetic budgets by block, with exact partial-block and mid-block entry
  fallbacks. Memory operations retain individual checks so a fault cannot consume
  later instructions or apply base writeback.

## Correctness evidence

Both final AWS runs passed **20 Rust tests**, including the compiled C fixture,
observable arithmetic checksum, permission/bounds failures, unchanged state on
failed writeback, SP alignment, reserved encodings and partial-block budget tests.
The existing 1,000 interpreter/translator comparison programs also passed.

Unicorn 2.1.4 independently decoded and executed **7,419 cases per engine** over
three seeds on each final AWS instance. Every case compared all 31 general
registers, SP, PC, NZCV, instruction count, 512 data bytes and 4 KiB of stack.
Coverage includes 32/64-bit overflow/carry edges, all 16 incoming flag patterns
against all condition codes, logical shifts, scalar addressing modes, writeback,
pairs and BL/RET. Both engines matched in every tested case.

Raw oracle receipts: [run 02](run-02/results/), [run 03](run-03/results/).
Inputs are deterministic, with seeds and SHA-256 digests in the receipts.
Unicorn is an independent test dependency; its implementation is not linked into
Sable. [Unicorn API](https://www.unicorn-engine.org/docs/tutorial.html),
[pinned release](https://github.com/unicorn-engine/unicorn/tree/2.1.4).

[Native Apple receipt](apple-oracle-with-checksum.json): the exact checked-in
hello, arithmetic and compiled C binaries ran through an owned iOS 27.2 Simulator
device (build 24B8117). Arithmetic emitted `406b8988792d0000`, representing
`50,000,005,000,000`; C emitted `fa23b45693b4658e`, representing
`0x8e65b49356b423fa`. The device was deleted and the private set contained zero
remaining devices. No Apple runtime or SDK was transferred to AWS.

The C fixture was also rebuilt with Linux Clang on each AWS host and its checksum
verified. Scalar checks do not validate atomics, NEON, memory ordering, threads,
signals, dynamic loading, kernel behavior or full architectural fault semantics.
Fault regressions compare Sable's engines; they do not independently prove ARM
fault behavior or hostile-code isolation.

## Benchmark protocol

Each final run used a new Ubuntu 24.04 `c7i.large` (2 vCPU, 4 GiB), Intel Xeon
Platinum 8488C, Rust 1.93.1 and Ubuntu's QEMU 8.2.2 package
`1:8.2.2+ds-0ubuntu1.18`. The two instances used the same Sable executable hash.
They could share underlying hardware; these are instance replications, not a
representative fleet sample. Host CPU metadata and kernel version are in stdout.

The controller pinned the benchmark process and its children to CPU 0. For each
fixture, it warmed each engine twice, then ran **31 complete randomized blocks**
containing interpreter, translator and QEMU. Each sample launched a new process
and included startup, file loading, translation and execution. Every recorded
sample had to exit 0 and emit the exact expected eight checksum bytes.

QEMU runs Linux ELF because its Linux userspace frontend does not implement the
Darwin loader/ABI. The fixture-specific repackager preserves computational
instructions, data layout and guest addresses. It changes only fixed assembly
syscall setup from Darwin X16/write/exit and `svc #0x80` to Linux X8/write/exit
and `svc #0`. No frameworks, dynamic linker, guest OS or network calls execute.
Thus this compares restricted process-level CPU execution, not equivalent platform
services or full iOS support. Sable retains its finite instruction budget; QEMU
runs without an equivalent instruction budget.

The corrected runs contain **372 recorded samples, zero failures**. Raw samples
are journaled before proceeding. Reported intervals use 5,000 paired bootstrap
resamples of the 31 blocks within each instance. They describe within-run sampling
uncertainty, not between-host or fleet-wide uncertainty. File caches were warmed;
this is not a controlled cold-storage experiment.

## Results

All times below are process-level wall-clock medians in milliseconds. Speedup is
QEMU median / Sable translator median; a value above 1 favors Sable.

| Run | Fixture | Interpreter | Sable native | QEMU | Speedup [paired 95% interval] |
| --- | --- | ---: | ---: | ---: | --- |
| 02 | Arithmetic + checksum | 383.257 | 8.028 | 11.082 | 1.380× [1.321, 1.449] |
| 03 | Arithmetic + checksum | 387.728 | 8.161 | 11.071 | 1.357× [1.310, 1.448] |
| 02 | Compiled C | 18.990 | 3.009 | 4.748 | 1.578× [1.553, 1.607] |
| 03 | Compiled C | 19.312 | 3.093 | 4.923 | 1.592× [1.548, 1.611] |

Sable's native p95 was 8.419/8.552 ms for arithmetic and 3.170/3.291 ms for C.
The corresponding QEMU p95 values were 12.460/12.458 ms and 5.056/5.088 ms.
Peak per-process RSS medians were approximately 2.2–2.3 MiB for Sable and
8.5–8.6 MiB for QEMU. RSS is not total host memory, incremental session cost or
achievable device density; these programs touch very little memory. GNU time CPU
durations have 10 ms resolution, so wall time is the useful measure here.

Full reports: [run 02](run-02/results/benchmark.json),
[run 03](run-03/results/benchmark.json). Raw samples:
[run 02 journal](run-02/results/benchmark.jsonl),
[run 03 journal](run-03/results/benchmark.jsonl).

## Exploratory run and what it taught us

[Run 01](run-01/results/benchmark.json) used the earlier emitter, before register
caching and block budgeting. Its C fixture had the same checked output and took
3.392 ms versus QEMU's 4.778 ms. The later native C medians were about 9–11% lower,
but this is an across-instance observation, not a randomized causal A/B estimate.

The exploratory arithmetic fixture emitted no checksum. Exit 0 plus empty output
could not validate QEMU's arithmetic result. Those timings are retained for audit,
and **excluded from the corrected performance claim**. Review caught this gap;
the `checksum` fixture now emits the value and all final samples check it.
The older S0 reference-interpreter speedups also remain historical measurements,
not competitive results.

## Reproduce and remaining gates

```sh
# Local correctness; Python oracle dependency stays outside the Rust runtime.
cargo test --release
python3 -m venv artifacts/oracle-venv
artifacts/oracle-venv/bin/pip install -r scripts/oracle-requirements.txt
cargo build --release --example oracle_batch
artifacts/oracle-venv/bin/python scripts/oracle.py
# x86_64 Linux:
artifacts/oracle-venv/bin/python scripts/oracle.py --engine jit

# Disposable paid AWS instance; never overwrite an existing experiment folder.
python3 scripts/aws-smoke.py run --extended --output evidence/s1/new-run
```

Each AWS run used a terminate timer and removed its own instances, volumes,
security groups and EC2 public keys. CLI readback found zero remaining resources
for [run 01](run-01/cleanup.json), [run 02](run-02/cleanup.json) and
[run 03](run-03/cleanup.json). Private recovery state and local SSH keys remain
outside Git. Source/fixture hashes and logs accompany each run.

This establishes a tested scalar execution milestone. Sable still has no dyld
binding/shared-cache support, Objective-C/Foundation/UIKit, Mach messages, thread
scheduler, graphics or app bundles. The 2–4× completed-iOS-workflows-per-dollar
goal remains unproven. The next compatibility work is dynamic process startup,
TLS and missing Darwin contracts, guided by an instrumented first-party probe.
