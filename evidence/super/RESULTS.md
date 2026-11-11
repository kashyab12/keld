# Scalar inheritance and Super2 execution

Sable executes original scalar superclass methods with inherited lookup and
`objc_msgSendSuper2`, including a superclass supplied by an original dylib.
[Supported profile and exclusions](../../docs/super.md). The actual UIKit app
still stops at its unsupplied UIKit dependency, before registration or execution
of its app/framework classes; zero interactions.

## What ran where

| Path | Result |
| --- | --- |
| Native Apple reference on ARM64 Mac | Twenty new programs and four prior controls passed on iOS 27.2 Simulator build 24B8117. This executes Apple's local runtime. |
| Sable interpreter on ARM64 Mac | Twenty hierarchy/direct-adoption CLI programs passed, with eight explicit depth rejections. All 67 Rust tests, project quick/full checks and 14,087 independent CPU cases passed. |
| Sable on x86 EC2 | Final source passed 73 Rust tests, 40 rebuilt hierarchy/control CLI executions, 16 expected depth rejections and 84,522 independent full-state CPU cases. |
| Actual original Notes UIKit executable in local Sable | Exit 125, empty stdout, required UIKit dylib not supplied, zero interactions. |

The [native receipt](native-reference.json) has 24 passes, delete exit 0 and zero
remaining owned devices. No Apple SDK, implementation, UIKit executable, video or
xcresult was transferred to EC2. The original native test programs are the same
checked-in Mach-O inputs used by Sable's Rust tests; Linux also rebuilds the source
with its own compiler. Native reference and translated execution are distinct.

## Contracts and regressions

The profile admits roots and direct children with no extra instance storage.
Registration resolves the graph after collecting every identity, independent
of declaration or library input order. Both ordinary and metaclass superref
slots are checked. Cycles, wrong-kind/unknown edges, changed RO layout, deeper
ancestry and missing parent libraries reject. Own code/metadata boundaries and
existing scalar-signature/cache checks remain mandatory.

Ordinary lookup searches own then parent tables. Super2 loads the actual guest
pair, checks current class/metaclass versus receiver, starts at the parent and
returns a guest IMP in x16. Guest BR, method instructions and RET execute with
real self, SEL, argument and caller-link state. The child then resumes its own
continuation. Instance/class overrides produce 49/51; inherited instance/class
calls produce 46/50; a nested normal/super flow produces 91. A leaf assembly IMP
captures scalar arguments/x8, self/SEL, frame, incoming x18, SP and actual LR.
Separate Sable probes check additional modeled scalar state, NZCV and every
pair-load/SVC/BR/IMP-return instruction-budget edge in both Linux dispatch modes.

Absolute/compact × legacy/chained × same-image/dylib × plain/constructor makes
sixteen positive programs. Four additional native controls prove direct C
protocol adoption remains separate from inherited methods. Eight third-level
programs are intentionally unsupported. Fresh Prepared runs, reversed library
input, mutable instance state, malformed stack pairs, nil/root Super2, wrong
current/receiver/SEL, foreign SVC origins, runtime stub changes, prepared/live
metadata mutation and code aliases have dedicated checks. Existing ordinary
message-to-nil behavior remains intact. The independent emitted-format reports
cover the original final class graphs, superrefs and method/fixup representations.

## Real legacy-linker gap

Cross-dylib legacy output initially failed locally with `dyld offset overflow`.
LLVM encodes a backwards bind cursor step as the unsigned 64-bit ULEB value
`0xfffffffffffffff0` (minus 16 modulo 64 bits). Sable's checked addition rejected
it. The corrected binding cursor uses that format's arithmetic but validates each
result against its selected mapped segment. All pointer writes remain bounded
and non-executable. Focused tests cover standalone ADD_ADDR, combined bind/add
and repeated skip encodings, plus invalid underflow/forward results and absent
segment selection. Rebase arithmetic and symbol/TLS addend checks are unchanged.
The final native cross-dylib legacy pass independently confirms acceptance of
the actual original LLVM output. No emitted stream was rewritten to avoid it.

## Native expectation correction

A native assumption test incorrectly expected the C conformance function to
search parent declarations. The [failed expectation](native-assumption-failed.json)
returned 21 in four controls, while its sixteen hierarchy programs and four old
controls passed. Its owned native device was deleted, zero remaining devices.
[Exact original failing source](native-assumption-source.m) and
[hashes](native-assumption-source-sha256.json) are retained. The corrected test
checks parent direct adoption is true, child direct adoption is false, while
inherited method lookup still works. Sable retains its original direct-adoption
semantics. This corrects a test expectation, not a native runtime failure.

## Reproduction and provenance

```sh
bash scripts/check-super.sh
cargo test --release --test super
cargo test --release --lib objc::hierarchy_tests
python3 scripts/apple-oracle.py --super --output artifacts/super-native.json
python3 scripts/aws-smoke.py run --extended --output artifacts/super-aws
```

[Source/fixture hashes](source-fixture-sha256.json) cover the original code,
checked-in inputs, reused leaf/startup sources and runners. Recipe 15 quick/full
checks passed and the recipe was published after the CLI hello verification.
The broader independent CPU corpus is retained as a regression check; no new CPU
instruction family is added in this change.

The [EC2 run](aws-01/run.json) used Ubuntu 24.04 on an x86-64 `c7i.large`,
two vCPUs, Rust 1.93.1 and Clang 18. [Raw stdout](aws-01/stdout.txt),
[stderr](aws-01/stderr.txt), [runtime hashes](aws-01/runtime-source-sha256.json)
and metadata/output receipts are retained. All 199 transferred runtime files
match the final checkout. The six oracle runs are 14,087 cases × three seeds ×
interpreter/JIT. The previous 16 protocol and 16 ObjC positive CLI programs and
eight unknown-selector rejections also passed. The existing CPU/QEMU benchmark
ran 186 correct process samples; it measures no superclass or app performance.
The graphics exposure inventory is retained separately; no rendering ran.

[Cleanup](aws-01/cleanup.json) verified zero remaining instances, volumes,
security groups and keys. The controller finished successfully; no owned native
or cloud process remains. [Verification summary](verification.json). CI rebuilds
and executes the same matrix on Linux/macOS; its exact-commit result is separate
from these local/cloud receipts.

The [actual app attempt](app-still-unsupported.json) now passes section parsing
and stops at its missing UIKit dependency. This does not establish registration
of its still-unsupported class layouts or successful framework startup. The
previous UI video remains Apple Simulator on the Mac. Deeper/dynamic graphs,
added ivars, allocation/ARC, +load/+initialize, categories, framework supply,
Mach IPC, FP/SIMD/aggregate ABI and guest threading remain open. There is no new
Sable UI recording, GPU rendering, microVM/concurrency test or competitor
measurement. GPU-free app execution and a 2–4× app advantage remain unproved.
