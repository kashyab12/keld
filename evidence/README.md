# Execution evidence

These are raw receipts from bounded experiments, not a complete simulator or
competitive benchmark. No Apple runtime, SDK, credentials, cloud account ID,
instance IP or SSH key is distributed here.

The [superclass/Super2 report](super/RESULTS.md) adds original inherited and
super-call execution with same-image/dylib parents. Native and Sable results,
failed expectations and the still-missing UIKit dependency remain separate.

The [static protocol report](protocols/RESULTS.md) separates native Apple
reference execution, Sable on Linux and the still-rejected UIKit app. It adds
name/identity/adoption checks and compiler-driven CPU coverage; it establishes
no graphical or competitor performance result.

The [S2 report](s2/RESULTS.md) adds original dynamic libraries, minimal LibSystem
ABI calls, immutable image sharing and reusable execution, with paired dispatch
and preparation measurements. Earlier phase receipts remain unchanged below.

The newer [S1 report](s1/RESULTS.md) covers scalar memory/flags, independent CPU
validation, compiled C and a QEMU comparison. The S0 measurements below describe
the initial implementation and are preserved as historical evidence.

## Corrected Linux run

[`aws-smoke/run.json`](aws-smoke/run.json) identifies Ubuntu 24.04 on an AWS
`c7i.large` in us-east-1. [`stdout.txt`](aws-smoke/stdout.txt) contains the CPU
inventory, test results, guest output and raw benchmark samples.
[`stderr.txt`](aws-smoke/stderr.txt) contains compiler/run diagnostics.
[`runtime-source-sha256.json`](aws-smoke/runtime-source-sha256.json) records the
runtime, tests and fixture file hashes corresponding to the corrected run.

Observed:

- Formatting and Clippy passed.
- 12 Linux tests passed: one layout test plus 11 integration tests, including
  1,000 generated programs compared between interpreter and native translator.
- Clang and LLVM rebuilt both ARM64 Mach-O fixtures byte for byte on Linux.
- The translated hello fixture printed `SABLE_ARM64_DARWIN_OK` and exited 0.
- The translated arithmetic fixture executed 30,000,006 guest instructions,
  produced checksum `50,000,005,000,000` in X9 and exited 0.
- All nine benchmark pairs matched the full guest CPU state, outputs, exit
  status, syscall count and remaining instruction budget.

[`cleanup.json`](aws-smoke/cleanup.json) records zero remaining active instances,
tagged volumes, security groups and keys, from AWS CLI readback after termination.

## Internal translation measurement

| Run | Interpreter median | Native translation median | Ratio |
| --- | ---: | ---: | ---: |
| Initial implementation | 336.01 ms | 17.93 ms | 18.74× |
| Corrected fault/loader handling | 379.80 ms | 18.05 ms | 21.05× |

Both runs used the same checksum workload on separate disposable instances of
the same SKU. Each has nine alternating-order pairs and includes fresh native
translation in every sample. Raw JSON: [initial](aws-smoke-initial/benchmark.json),
[corrected](aws-smoke/benchmark.json). The reference interpreter fetches and
decodes on every step; it is deliberately simple. This ratio measures that
dispatch overhead and direct native branch linking on a tiny arithmetic loop.

These are smoke measurements. They do not compare QEMU, Macbox, Limrun, Apple
Simulator or real apps, and do not establish a 2–4× product improvement. There
is no confidence interval, randomized host sample, UI workload, memory-density
result, ARM Linux result or all-in cost result. The variation in interpreter
timing between hosts is another reason to retain raw samples and avoid a product claim.

The initial run predates fail-closed optional-command handling and consistent
budget precedence on fault paths. Its logs and cleanup receipt are preserved
under [`aws-smoke-initial/`](aws-smoke-initial/). The corrected run exercises those
regressions too. Both cloud hosts were terminated.

## Native Apple oracle

[`apple-oracle.json`](apple-oracle.json) records running the same checked-in
fixtures through an owned temporary CoreSimulator device on the development Mac:
iOS 27.2, build 24B8117. Hello produced identical stdout and exit 0; compute exited
0. The device was deleted, and listing the private device set returned zero devices.

Reproduce on an Apple silicon Mac with Xcode and the supported runtime installed:

```sh
python3 scripts/apple-oracle.py
```

This confirms that the fixture is accepted by Apple's Simulator loader and that
the small hello behavior agrees. The compute fixture does not print its registers
on Apple; its native exit alone does not independently prove our checksum or the
shared decoder's semantics. Broad native differential CPU validation remains S1.
