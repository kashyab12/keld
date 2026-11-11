# Ordinary ownership and guest uppercase

Sable completes a bounded Foundation console workflow with original providers:
the guest allocates string storage, converts actual ASCII bytes to uppercase,
keeps the UTF-8 pointer alive through use, drains ordinary autorelease pools and
runs guest dealloc methods. Counts, nested pools and callback state are private
to each execution. [Supported contract](../../docs/ownership.md).

| Input | Where it ran | Observed result |
| --- | --- | --- |
| Retained SDK-linked Foundation minimal | Sable interpreter on ARM64 Mac, with original providers | `SABLE FOUNDATION`, exit 0, 463 instructions |
| Eight rebuilt ARC/ownership variants | Sable interpreter and JIT on x86-64 EC2 | All 16 executions passed |
| Actual UIKit Notes executable | Sable interpreter on Mac | Missing UIKit, exit 125, zero interactions |
| Earlier native UI recording | Apple's Simulator on Mac | Native reference only; no new UI recording in this gate |

The [retained executable receipt](retained-sdk-minimal.json) records the same
input SHA256 before and after execution. It was neither rebuilt nor executed
natively in this gate, and it was not transferred to Linux. The framework
providers match our checked-in original binaries. This is a supported console
slice, not execution of Apple's Foundation implementation.

The [EC2 run](aws-01/run.json) completed on Ubuntu 24.04 x86-64 `c7i.large`, two
vCPUs, Rust 1.93.1 and Clang 18. All 101 Rust tests passed, including complete
outcome comparisons in both JIT indirect-dispatch modes, every budget prefix
around services/callbacks, private fresh-run state and precise callback write
fault comparisons. [Raw stdout](aws-01/stdout.txt), [stderr](aws-01/stderr.txt).

Four absolute/compact × legacy/chained combinations each rebuild an ARC client
and a separate ordinary ownership observer. The16 Linux CLI outputs match exact
expected bytes. The observer checks nested pools, a retained object surviving an
inner pool, ordinary return claims, nil/immortal handling, specialized x20 input,
guest dealloc order, constructor and TLV state. Four independent format reports
inspect sixteen executable/framework inputs.
[Linux outputs and metadata](aws-01/results/ownership/).

The implementation checkpoint passed 94 local Rust tests and eight rebuilt CLI
programs; all 24 checked-in original binaries match that final local rebuild.
After the continuation correction, all nine focused ownership tests and format/
all-target Clippy checks passed on the final source. These are separate runs,
not a claim of a final 95-test Mac suite.
[Local summary](local-verification.json), [local format records](local-formats/).
Recipe 19 was published after the integrated original hello printed
`SABLE_ARM64_DARWIN_OK` with exit 0.

Source review found a missing validation boundary after guest dealloc returns.
The correction validates current metadata/cache/protocol snapshots before
clearing the callback or restoring its caller, and independently before draining
queued work. A normal Memory regression checks modified metadata/cache after
disposal, pending count changes and final pool removal, with no ownership-state
or arena effects on rejection. [Correction receipt](continuation-correction.json).
[Earlier development failures and corrections](development-failures.json) remain
recorded. No new arithmetic, SIMD or memory instruction was added.

All 459 captured runtime/test/input files match measured commit `1d7be04` byte for
byte. Documentation updates are separate from that measured source snapshot.
[Capture verification](capture-verification.json),
[capture manifest](aws-01/runtime-source-sha256.json).
All 38 published source/fixture hashes also match.
[Source manifest](source-fixture-sha256.json).

Six independent CPU oracle runs passed 84,702 full-state cases. The existing
CPU/QEMU benchmark retained 186 correct samples, zero failures, with recomputed
medians/p95 matching its raw journal. It measures no Foundation or UI performance.
Prior string, payload, instance, superclass and protocol CLI checks also pass.
[Verification summary](verification.json), [raw CPU samples](aws-01/results/benchmark.jsonl).

The controller exited0, and AWS readback verified zero remaining active owned
instances, tagged volumes, security groups and keys. [Cleanup](aws-01/cleanup.json).
No native Simulator device or microVM was created in this gate. The exposure
[inventory](aws-01/results/graphics-inventory.json) records no exposed GPU nodes;
no renderer was exercised and this is no GPU-free UI result.

The [actual Notes attempt](app-still-unsupported.json) still rejects missing UIKit
before any interaction or frame. General NSString/Unicode, NSObject lifecycle,
optimized ARC return handoff, strong/weak fields, reentrant lifecycle, threads and
UIKit remain unsupported. UI FPS, input latency, concurrent iPhone capacity and
a 2–4× competitor app advantage remain unmeasured. The earlier
[video](../recordings/README.md) is an Apple Simulator reference.

```sh
bash scripts/check-ownership.sh
python3 scripts/aws-smoke.py run --extended --output artifacts/ownership-aws-new
```

Only original source and original LLVM fixture binaries are distributed here.
Earlier missing-pool and missing-UIKit receipts retain their historical outcomes.
