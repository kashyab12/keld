# Constant-string execution

Sable executes original non-ARC constant-string programs through genuine original
Foundation/CoreFoundation classes, an inherited guest UTF8String getter and guest
strlen/strcmp/write. [Exact profile](../../docs/strings.md).
At this checkpoint, the ARC Foundation uppercase workflow and UIKit app remained
unsupported. The later [ownership gate](../ownership/RESULTS.md) advances the
console workflow; the observations below retain their original scope.

All eight variants passed Apple's native iOS27.2 reference, alongside four prior
controls. [Native receipt](native-reference.json): delete exit0, zero owned devices
remaining. This uses Apple's installed runtime locally on the Mac, separately
from the original methods executed by Sable.

All 86 local Rust tests passed. Recipe18 quick/full passed and was published
after manual original-hello verification. Eight rebuilt local string programs
passed, with four independent format reports inspecting twenty original
executable/library inputs. All 28 checked-in LLVM binaries match the final rebuild;
all 46 [source/fixture hashes](source-fixture-sha256.json) match.
[Local summary](local-verification.json), [format records](local-formats/).
The unchanged 14,117-case [CPU oracle](oracle-interp.json) also passed.

One unit regression corrupts post-fixup isa/flags/pad/length and owning ASCII
bytes, and checks the exact scalar pointer-return scope. Three integration tests
cover all compiled variants, constructor reads, reversed library inputs, private
repeated execution and budget recovery; malformed sections, read-only flags,
missing/wrong-version/alias providers; prepared mutations, code aliases, writable
permission changes, actual getter perturbation, guest write faults and forged
constant receivers. Linux additionally compares full outcomes in both JIT
indirect-dispatch modes.

[An initial probe failure](initial-local-gap.json) is retained: the compiler
emitted an unsupported SIMD copy for a local fixed output banner. The banner is
now static data. No getter, string comparison or data-dependent result was
replaced, and SIMD remains unsupported. No new CPU instruction was added.

The [first EC2 attempt](aws-01/run.json) failed a new negative test because it
incorrectly demanded identical interpreter/JIT fault messages. Both rejected the
null access; their existing diagnostics differ. The correction checks the expected
rejections and adds direct complete CPU/memory comparisons at the read-only write
fault in both JIT modes. No runtime implementation or fixture changed.
[Failure/correction summary](failed-attempt.json), [failed stdout](aws-01/stdout.txt).
Cleanup verified zero resources; the failed run is retained. Local tests and
format/lint passed after this test correction.

The [corrected EC2 run](aws-02/run.json) completed successfully on Ubuntu 24.04
x86-64 `c7i.large`, two vCPUs, Rust 1.93.1 and Clang 18. All 92 Rust tests and all 16
rebuilt string CLI executions passed. Four independent Linux format reports
inspect twenty executable/library inputs. Full positive outcomes and the precise
read-only write-fault CPU/memory state match both JIT dispatch modes.
[Raw stdout](aws-02/stdout.txt), [stderr](aws-02/stderr.txt),
[execution/format files](aws-02/results/strings/).

All 427 transferred runtime/input files match the final checkout byte for byte;
[the capture manifest](aws-02/runtime-source-sha256.json) is retained. Comparing
the failed and corrected capture manifests confirms the sole change was the new
Rust test file. No runtime implementation, framework, fixture or runner changed.
The six independent CPU oracle runs passed 84,702 full-state cases. Prior 128
payload, 32 instance, 40 superclass and 16 protocol positive CLI outputs also pass.
The existing CPU/QEMU regression benchmark has 186 correct samples, zero failures;
it measures no Foundation string or application performance. Graphics exposure
inventory is retained; no renderer ran.

Both the [failed cleanup](aws-01/cleanup.json) and
[corrected cleanup](aws-02/cleanup.json) queried zero active instances, volumes,
security groups and keys. The corrected controller exited 0. Native owned devices
are also gone. [Verification summary](verification.json). CI rebuilds the matrix
on Linux/macOS; its exact-commit result is separate from these receipts.

The [retained ARC minimal attempt](arc-minimal-still-unsupported.json) now gets past
the previously missing framework dependency with original providers supplied;
it exits125 at unresolved objc_autoreleasePoolPop, with no output and no completed
Foundation workflow. The [actual Notes attempt](app-still-unsupported.json) remains
missing UIKit with zero interactions. These are local attempts; no Apple-linked
executable or framework was transferred to EC2.

The [native ownership research summary](research-native-summary.json) records six
Apple references, separate from Sable: unchanged ARC minimal, constant/interior
pointer lifetime and nested ownership/return-claim controls on macOS/iOS. Its
original native-only observer sources are published. Those results do not imply
Sable has ARC, pools, deallocation, optimized ownership handoff or uppercase.

No UIKit/GPU-free rendering, new microVM/density run or competitor workflow
benchmark is part of this gate. The earlier video remains an Apple Simulator
reference. A 2–4× app advantage remains unproved.
