# Ordinary ownership and ASCII uppercase

Sable's next bounded original-framework gate adds per-run strong counts, nested
ordinary autorelease pools, and a guest dealloc continuation. Original Foundation
code allocates an NSString with a 32-byte checked layout and up to 64 extra bytes,
transforms actual ASCII input into that storage, appends NUL, stores its byte
pointer/length, and calls ordinary objc_autorelease. UTF8String remains a guest
load. There is no engine implementation of uppercase, strlen, strcmp or NSString.

The headerless client is compiled with ARC and retains the
`objc_returns_inner_pointer` declaration. Its emitted ordinary fallback path is
allocation count1 + producer queue1, claim count2, retainAutorelease count3 +
queue2, specialized release count2, then pool drain to zero and guest dealloc.
Both claim helper names have ordinary retain fallback semantics. Optimized
`objc_autoreleaseReturnValue` production is rejected. Register-specific bridges
cover only the actual emitted release_x20..x24 and retain_x22 forms in these
fixtures; release_x20 takes x20 even when x0 is unrelated. The void calls do not
promise preservation of caller-clobbered registers, vectors or NZCV.

Ownership providers must supply the complete original export/SVC/RET set. Export
ownership, exact stub instructions, and SVC origin are checked. Partial sets,
forged bridges, and unregistered origins reject. Providers still require the
existing strict dependency, version, metadata and signature checks.

Each execution uses 256 quarantined slots (512 bytes each since the FirstFrame profile). Allocations have
live strong counts (cap 1,000,000), finalizing and retired states. There are at most
32 active pools, 1024 pending releases, and one active dealloc continuation.
Ledger, pool and queue capacity is reserved before guest execution and never
grows beyond those limits. Nil and exact registered immutable constants are
explicit ownership cases. Unknown, interior, stale and forged identities reject.
Retain+enqueue preflights both effects; pool pop validates its top token, all
queued identities/counts, and required hooks before draining. Tokens are never
reused during a run. Pool draining is LIFO. Lifecycle reentrancy is explicitly
unsupported, including direct dealloc messages, allocation/ownership calls from
a dealloc callback, and disposal of any other object.

At the last release the plan resolves a checked ordinary `v16@0:8` dealloc method
before changing the count. Prepared::drive enters that IMP in the same Process,
Memory, output buffers, clock and remaining instruction budget. It saves the
service resume and ABI-preserved state, supplies self/SEL and an unmapped private
return sentinel, and passes that active sentinel to compiled dispatch too. It
checks balanced SP and exactly one disposal of the expected finalizing object,
validates current metadata/cache/protocol snapshots before clearing the callback
or restoring the caller, and continues any pool drain. Drain entry independently
validates those snapshots before changing queued releases, counts or pools.
Guest object_dispose scrubs the whole slot and retires its identity. No fresh Prepared/run, startup CPU reset or
recursive host callback is used. Successful execution requires zero live owned
objects, pools, queued releases and callbacks. Old non-ownership instance-service
programs keep their explicit create/dispose behavior.

`Image::link_main` and CLI `run ... --main` explicitly opt into a checked LC_MAIN
call with guest argc/argv/envp/apple arguments and return status, even with no
initializers. Default `Image::link` and CLI freestanding behavior are unchanged.
No dummy constructor or synthetic guest exit is added.

The four absolute/compact × legacy/chained combinations each include an ARC
uppercase client and a separate ordinary ownership observer. The observer uses
real guest dealloc counters/order, a retained object surviving an inner pool,
claim fallback, nil/immortal controls, specialized x20 input, a constructor and
TLV data. Tests compare three fresh runs, complete outcomes and arena scrubbing;
exhaust all instruction-budget prefixes around services and callback returns;
and perturb bridge exports/stubs/origins, getter, transform, data and dealloc.
Linux x86_64 additionally compares both JIT dispatch modes, including exact
CPU/memory state at a guest write fault inside dealloc. These Linux paths require
Linux execution; an ARM Mac only executes the interpreter.

```sh
bash scripts/check-ownership.sh
cargo test --release --locked --lib ownership
cargo test --release --locked --test ownership
```

These are original LLVM-built headerless fixtures. A retained exact SDK-linked
minimal binary is a separate local read-only input, never a checked-in or cloud
fixture. The older unresolved-pool receipt remains a record of the previous
constant-only gate. Native ownership observers remain external references, not
newly executed Sable or native evidence.

This slice supports ASCII input length 0..63 and ordinary ownership only. It does
not implement general NSString/Unicode, NSObject lifecycle, optimized ownership
handoff, weak/strong fields, C++ objects, threads, UIKit or rendering. No density,
concurrency or competitor-performance claim follows from this work.
