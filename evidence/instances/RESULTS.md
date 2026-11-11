# Fixed-layout instance execution

Sable now creates and disposes real bounded guest instances, with object-pointer
method arguments/results and per-run lifetime tracking.
[Contract and limits](../../docs/instances.md).

Native reference: all sixteen instance variants and four prior controls passed
on iOS 27.2 Simulator build 24B8117. The [receipt](native-reference.json) has delete
exit 0 and zero remaining owned devices. This runs Apple's local runtime on the
Mac. It is separate from Sable's execution with our original runtime libraries.
The actual UIKit app still has no Sable interactions.

The original instances use an inherited factory, real identity/choice/comparison
IMPs and an overridden/super method returning 49. Requested extra bytes start
zero and remain separate across live objects; disposal/recreation checks fresh
bytes without asserting native address reuse or reading freed native objects.
A guest constructor carries a live object and mutable extra byte into main, then
main disposes it. Scalar TLV state coexists and stays private across runs. An
original assembly caller checks native callee-saved/platform/frame/link state.

## Local checks and boundaries

All 75 Rust tests and Recipe 16 quick/full checks passed on this ARM64 Mac.
The recipe was published after the original hello verification. The full
14,087-case independent CPU corpus also passed; this feature adds no new CPU
instruction family. The final sixteen rebuilt instance outputs passed. Both
method/fixup formats, same-image/dylib parents and constructor/plain execution
are represented. The exact 36 original binaries match the final local rebuild.

Three unit tests cover strict Q/@ method signatures while keeping protocol
signatures Q-only; fixed arena layout, overlap/total-memory limits; private state,
zeroing, stable arena/ledger backing pointers, capacity and lifetime. Five
integration tests exercise compiler programs and reset after budget errors;
all modeled scalar registers/NZCV/SP/LR and every creation/disposal SVC/RET budget
edge; forged/interior/static/double disposal, isa changes and stale normal/Super2
messages; saturation and no retired-address reuse; opaque pointer-valued arguments,
foreign SVCs and changed stubs. Linux additionally compares complete outcomes in
both JIT dispatch modes and checks raw backing pointers remain stable.

The arena is 32 KiB, 256 slots ×128 bytes with 64 requested extra bytes maximum.
Its allocation ledger reserves all records once at setup. Creation/disposal
perform bounded writes and state changes without per-object host allocation,
mapped-region growth or pointer-table relocation. Total-allocation exhaustion
returns nil, even after disposal, because slots stay quarantined until run reset.
This is a supported resource limit, not native allocator capacity or app density.
No per-object memory protection or cross-tenant isolation is claimed.

## Reproduce and inspect

```sh
bash scripts/check-instances.sh
cargo test --release --test instances
cargo test --release --lib objc::instances::tests
python3 scripts/apple-oracle.py --instances --output artifacts/instance-native.json
python3 scripts/aws-smoke.py run --extended --output artifacts/instance-aws
```

[Source/fixture hashes](source-fixture-sha256.json) cover the original inputs,
reused startup/TLV/ABI helpers and runners. The independent emitted-format
receipts identify exact @/Q encodings, class graphs, superrefs and fixup/method
representations. Native execution uses the original checked-in programs; Sable's
Linux run additionally rebuilds them with its compiler.

The [EC2 run](aws-01/run.json) passed 81 Rust tests and 32 rebuilt instance CLI
executions across interpreter/JIT. Eight format reports inspect twenty emitted
program/library inputs. Both Linux dispatch modes are checked in the Rust tests,
including stable pointers and complete outcome equality. All 239 transferred
runtime files match the final checkout. [Raw stdout](aws-01/stdout.txt),
[stderr](aws-01/stderr.txt) and [runtime hashes](aws-01/runtime-source-sha256.json)
are retained. The host was Ubuntu 24.04 on x86-64 `c7i.large`, two vCPUs, Rust
1.93.1 and Clang 18.

Six oracle runs total 84,522 full-state cases (14,087 × three seeds × both
engines). Prior 40 superclass, 16 protocol and 16 root-class positive CLI
executions passed, along with 16 depth and eight unknown-selector rejections.
The existing CPU/QEMU benchmark ran 186 correct process samples; it measures no
object allocation or app advantage. Graphics exposure inventory is retained;
no renderer ran.

[Cleanup](aws-01/cleanup.json) verified zero instances, volumes, security groups
and keys. The controller completed successfully; no owned native/cloud process
remains. [Verification summary](verification.json). CI rebuilds the matrix on
Linux/macOS; its exact-commit result is separate from these local/cloud receipts.
No Apple implementation or SDK is distributed with these original probes.
The [actual app attempt](app-still-unsupported.json) remains exit125, missing UIKit
dependency, empty stdout and zero interactions before app class registration or
framework instructions. No +alloc/+init automation, ARC/retain-release,
destructors, added ivars, general allocator, graphics, microVM, concurrency or
competitor benchmark is added. The previous app video is native Apple Simulator
on the Mac. GPU-free app execution and a 2–4× app advantage remain unproved.
