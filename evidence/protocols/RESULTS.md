# Static Objective-C protocol execution

Sable registers original static protocols and implements name/identity lookup
and direct class conformance. This adds bounded protocol-list section handling.
The actual UIKit executable now stops at `__objc_superrefs`, with zero Sable app
interactions. [Runtime contract and exclusions](../../docs/protocols.md).
The app's remaining protocol/class layouts and dependencies still require
validation after section parsing.

## What ran where

| Path | Result |
| --- | --- |
| Native Apple reference on this ARM64 Mac | Eight protocol variants and four prior controls passed on iOS 27.2 Simulator build 24B8117. This executes Apple's local runtime. |
| Sable interpreter on this ARM64 Mac | Eight protocol variants passed. The 59-test Rust suite, project quick/full checks and 14,087 independent CPU cases passed. |
| Sable on x86 EC2, attempt 01 | All 64 then-current Rust tests passed, including checked-in protocol fixtures. Fresh Ubuntu Clang 18 output failed at unsigned-bitfield instruction `0x531e7500`, so the run failed. |
| Sable on x86 EC2, attempt 02 | Final source passed 65 Rust tests, 16 freshly rebuilt protocol executions across both engines and 84,522 independent full-state CPU cases. |
| Actual original Notes UIKit executable in local Sable | Exit 125, empty stdout, unsupported `__objc_superrefs`, zero interactions. |

The [native receipt](native-reference.json) has twelve passes and delete exit 0,
zero remaining owned devices. Native execution uses the same checked-in original
Mach-O programs and the Mac's supplied Apple runtime; Sable runs them with our
explicit original runtime libraries. No Apple implementation or UIKit executable
was sent to Linux. The earlier UI video remains an Apple Simulator reference on
the Mac, not Sable running an iPhone on EC2.

## Behavioral and boundary checks

The original program defines two distinct protocols with the same scalar method
requirement. It checks literal/lookup identity, stable names, absent names,
positive/negative declared adoption, null edges and a real ARM64 method result
of 42. A class implementing the method without adoption is nonconforming; a
class declaring adoption without the method is conforming. Descriptions have
null IMPs and never enter method dispatch. Protocol records remain opaque;
sending an ordinary message to them explicitly fails.

Absolute/compact method lists × legacy/chained fixups × plain/constructor access
make eight programs. An original assembly caller checks native callee-saved,
platform, frame, stack and link state. Sable probes check every modeled scalar
register, NZCV and each service-call instruction-budget edge. Tests also cover
fresh prepared-run state, reordered library input, nearby mutable `__data`,
malformed protocol fields/type arrays/descriptions/identities, ownership/caps,
metadata/code aliases, foreign private traps and prepared/live mutation. Linux
Rust tests additionally compare complete outcomes in both JIT dispatch modes.
The independent metadata inspector checks the final emitted records in both
compiler environments. Cross-image protocol consumer execution is not separately
proved by these fixtures.

Two compiler-driven CPU prerequisites are implemented directly: TBZ/TBNZ and
UBFM with its unsigned extraction/insertion aliases. The oracle includes 1,024
bit-branch cases, all 5,120 valid unsigned-bitfield immediate pairs across both
widths, plus eight input edges for the exact Ubuntu instruction that failed.
It compares all scalar registers, SP, NZCV, PC, instruction count, data and stack
against Unicorn 2.1.4. Reserved width/N/immediate encodings reject. Signed and
merging bitfield operations remain unsupported. The expanded test-only batch
limit is bounded at 20,000 cases; the runtime has no Unicorn dependency.

## Reproduce and inspect

```sh
bash scripts/check-protocols.sh
cargo test --release --test protocols
cargo test --release --lib objc::protocol::tests
python3 scripts/apple-oracle.py --protocols --output artifacts/protocol-native.json
python3 scripts/aws-smoke.py run --extended --output artifacts/protocol-aws
```

[Source/fixture hashes](source-fixture-sha256.json) cover original inputs, binaries
and runners; the EC2 runtime manifest covers the Rust source/tests too. The
[failed EC2 run](aws-01/run.json), [stdout](aws-01/stdout.txt),
[stderr](aws-01/stderr.txt) and [cleanup](aws-01/cleanup.json) are retained.
Its checked-in binaries passed; rebuilding with a different compiler exposed a
missing instruction, which was added rather than changing the Bool declaration
or accepting an unexecuted program.

The [successful EC2 run](aws-02/run.json) used Ubuntu 24.04, x86-64
`c7i.large` with two vCPUs, Rust 1.93.1 and Clang 18. [Raw stdout](aws-02/stdout.txt),
[stderr](aws-02/stderr.txt), [runtime hashes](aws-02/runtime-source-sha256.json)
and rebuilt metadata/output receipts are retained. All 146 transferred runtime
file hashes match the final checkout. Six oracle runs are 14,087 cases × three
seeds × interpreter/JIT, totaling 84,522. Sixteen previous ObjC positive CLI
executions and eight explicit unknown-message rejections also passed. The
existing CPU/QEMU benchmark ran 186 correct samples; it contains no protocol or
app performance measurement. The graphics inventory records no exposed GPU
nodes and an unbound AWS display-class PCI device; it is exposure evidence,
not a rendering test.

Both disposable AWS attempts have verified zero remaining instances, volumes,
security groups and keys; [final cleanup](aws-02/cleanup.json). The second
bootstrap had one transient DescribeInstances NotFound read, retried without
replaying the launch; its raw read-error journal is retained. No owned native or
cloud process remains. [Verification summary](verification.json). CI rebuilds
and executes the matrix on Linux/macOS; its final-commit result must be read
separately from these local/cloud receipts.

No new microVM, density, rendering or competitor measurement belongs to this
change. The previous 150-guest CPU result remains a separate workload. General
inheritance/super, inherited/dynamic/optional protocol forms, categories,
allocation/ARC, +load/+initialize, framework supply, Mach IPC, FP/SIMD and guest
threads remain open. GPU-free UIKit execution and a 2–4× app advantage remain
unproved. [Actual app rejection](app-still-unsupported.json).
