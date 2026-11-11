# Q payload field execution

Sable executes real getter, void setter and superclass method bodies on validated
Q payload layouts, with instance creation derived from class RO size.
[Profile and boundaries](../../docs/payload.md).

The [native reference](native-reference.json) has 64 payload program passes and
four prior controls on iOS 27.2 Simulator build 24B8117. Delete exit 0, zero
remaining owned devices. This runs Apple's runtime locally on the Mac, separately
from Sable and our explicit original runtime libraries. No UIKit UI ran here.

Four root/child size pairs (16/24,8/16,64/64,16/64) combine with absolute/compact
methods, legacy/chained fixups, same-image/dylib parents and constructor/plain
execution. Programs check original field loads/stores, super sums, actual payload
and requested-extra offsets, zero/isolation, disposal/recreation and shared
constructor/main/TLV state. The normal sum is 42; larger field sets have separately
computed expected sums. No native raw isa, allocator padding/reuse or freed bytes
are compared.

## Local verification

All 82 local Rust tests passed. Recipe 17 quick/full checks passed and the recipe
was published after the original hello check. All 64 locally rebuilt payload
programs passed Sable's interpreter. The 144 original checked-in Mach-O/dylib
files match the final local rebuild. Thirty-two independent format reports
inspect 80 executable/library inputs and their actual emitted cells, layouts,
method encodings and fixup representations.
[Local summary](local-verification.json), [format records](local-formats/),
[source/fixture hashes](source-fixture-sha256.json).

Three new unit tests check layout ownership, exact rows/types/cells/ranges,
contiguous coverage, inherited prefixes, collection order and allocation-ledger
spans derived from every tested class size. Three integration tests check all
compiled variants, private repeated runs and recovery after budget failure;
prepared/live metadata changes and instruction aliases; actual void-setter
side effects and every lookup/store/return budget edge. Linux additionally checks
full interpreter/JIT outcomes in both dispatch modes.

LDRSW literal has a precise memory/flags/budget regression and 30 new independent
CPU cases. The [local Unicorn corpus](oracle-interp.json) passed all 14,117 cases,
comparing full modeled scalar state and memory. Its signed literal cases include
both signs, XZR destinations and forward/backward data. Other signed load
addressing families remain unsupported.

## Reproduce

```sh
bash scripts/check-payload.sh
cargo test --release --test payload
cargo test --release --lib objc::payload::tests
python3 scripts/apple-oracle.py --payload --output artifacts/payload-native.json
python3 scripts/aws-smoke.py run --extended --output artifacts/payload-aws
```

## EC2 verification

The [EC2 run](aws-01/run.json) completed successfully on Ubuntu 24.04 x86-64,
`c7i.large` with two vCPUs, Rust 1.93.1 and Clang 18. All 88 Rust tests and all
128 rebuilt payload CLI executions passed across interpreter/JIT. Thirty-two
Linux format reports inspect 80 executable/library inputs. Both JIT dispatch
modes have full-outcome comparisons in the Rust tests.
[Raw stdout](aws-01/stdout.txt), [stderr](aws-01/stderr.txt),
[format and execution files](aws-01/results/payload/).

All transferred runtime code, tests and original binary inputs match the final
checkout: 385 of 386 captured paths match byte for byte. The sole later change
is a [documented README clarification](post-capture-document.json), replacing
its old no-added-storage phrase; no executable source or input changed.
The original [capture manifest](aws-01/runtime-source-sha256.json) is preserved.
All 161 source/fixture hashes also match.

Six independent oracle runs passed 84,702 full-state cases: 14,117 × three seeds
× both engines. Prior 32 instance, 40 superclass, 16 protocol and 16 root-class
positive CLI executions passed; sixteen depth and eight unknown-selector controls
rejected as intended. The existing CPU/QEMU benchmark has 186 correct process
samples and zero failures. It does not measure object allocation, app work,
concurrent sessions or competitor performance. Graphics exposure inventory is
retained; no renderer ran.

[Cleanup](aws-01/cleanup.json) queried zero remaining active instances, volumes,
security groups and keys. The controller exited 0. Native cleanup is also
complete, with no owned device left. [Verification summary](verification.json).
CI rebuilds the matrix on Linux/macOS; its exact-commit result is separate from
these receipts.

A [local compiler-driven gap](local-literal-gap.json) is retained: split-dylib
ivar access initially failed at LDRSW literal. Signed literal support now uses
real sign extension and existing memory/budget guards; the offset load was not
rewritten or replaced with a synthetic field access. No new runtime query API
was added.

The [actual UIKit attempt](app-still-unsupported.json) remains missing-framework blocked before registration and
execution, with zero interactions. No rendering, GPU-free app, new concurrency
or competitor performance claim follows from these scalar programs.
