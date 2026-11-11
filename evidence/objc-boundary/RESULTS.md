# Objective-C metadata/code boundary correction

Independent review reproduced a P2 loader-validation defect in `d318d31`.
A malformed second instruction-section declaration could overlap compact method
metadata, making `contains_pc(table)` true and admitting a metadata address as
an IMP. This did not demonstrate a host escape or successful execution from
those bytes. Unmodified positive fixture receipts remain valid.

## Reproduction and correction

The regression enlarges the original selector-stub section through the existing
64-byte compact table. A second variant changes the first IMP displacement to
-16, pointing back to the table header. On the unmodified implementation,
linking accepted the alias; [before-fix output](before-fix.txt) records the
regression failing. The [after-fix output](after-fix.txt) records it passing.
The independent [review receipt](review-reproduction.json) also records
metadata classified as code and a metadata IMP accepted before the fix. Guest
code from malformed metadata was never executed.

The image parser now compares retained ObjC metadata with **all** collected
instruction ranges before returning an object. The check uses complete ranges,
not an exception for a particular section name or flags on the metadata section
alone. Preparation applies the same checked interval rule to registered metadata
snapshots, preventing a caller from reintroducing an alias through public
`Image` fields. Range-end arithmetic is checked for overflow.

The regression covers both fixup formats, the original selector-stub name and
an arbitrary instruction-section name, both section declaration orders, and
both unchanged/metadata-pointing IMP cases: sixteen malformed file inputs.
All reject before registration. It also checks public prepared-image aliases,
both native dispatch modes' preparation paths on Linux, and a range overflow.
Existing compact RX-write/branch rejection, exact fault state, mixed-format,
scalar ABI, constructors, budgets and fresh-run checks remain in the suite.

## Verification

| Check | Result |
| --- | --- |
| Before-fix local regression | Failed because malformed alias was accepted. |
| After-fix local regression | Passed for all sixteen malformed file variants plus preparation/overflow cases. |
| Local project checks | Recipe 13 quick/full passed, including 51 Rust tests and both original method-format fixture matrices. |
| Final x86 EC2 | Passed 57 Rust tests including the new regression, sixteen positive ObjC CLI executions, eight expected unknown-message rejections and 47,610 independent CPU oracle cases. |

No original fixture bytes or native runtime behavior changed. The prior
[native reference](../objc-compact/native-reference.json) is retained; no new
Simulator/native Apple execution was performed for this validation-only fix.
The [last actual UIKit attempt](../objc-compact/app-still-unsupported.json)
still has zero interactions at unsupported protocol metadata. No framework,
GPU-free UI, new microVM capacity or competitor/app-performance result follows.

Reproduce from the repository root:

```sh
cargo test --release --test objc metadata_instruction_aliases_reject_before_registration_and_preparation
bash scripts/check-objc.sh
python3 scripts/aws-smoke.py run --extended --output artifacts/objc-boundary-aws
```

The [final EC2 receipt](aws-01/run.json), [raw stdout](aws-01/stdout.txt),
[stderr](aws-01/stderr.txt), [runtime hashes](aws-01/runtime-source-sha256.json)
and [verification summary](verification.json) are retained. The run used
Ubuntu 24.04 x86-64 on an owned c7i.large, two vCPUs, Rust 1.93.1 and Clang 18.
Runtime source hashes match the final checkout. Linux exercises both JIT dispatch
modes in the Rust tests; CLI/oracle execution uses the default JIT mode.
[Cleanup](aws-01/cleanup.json) verified zero active instances, volumes, security
groups and keys. No owned test processes remain. The driver's pre-existing CPU
microbenchmark is retained without deriving an app/competitor claim.
