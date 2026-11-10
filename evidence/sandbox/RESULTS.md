# Sable executes inside jailed Firecracker microVMs

Observed 2026-11-10. Two fresh AWS hosts completed **124 fresh microVM jobs and
62 unisolated process controls**, with checked output on every successful sample.
This establishes a working containment experiment for Sable's current CPU/runtime
fixtures. It is not an iPhone app benchmark or an adversarial escape audit.

## Final host results

Each job runs both original initializer programs and a 200,000-round chained C
workload through ARM64-to-x86 native translation. Output must match the
independently Apple-checked constructor/argument stream and checksum. The same
static Sable executable runs in the process control and microVM, with identical
SHA-256 across the two successful hosts.

Run 05 includes per-job preparation and cgroup/jail cleanup:

| Profile | Samples | Median complete job | p95 complete job | Maximum cgroup charge | Maximum sampled VMM PSS |
| --- | --- | --- | --- | --- | --- |
| Unisolated process control | 31 | 11.0 ms | 12.4 ms | Not measured | Not measured |
| 128 MiB guest / 1 vCPU | 31 | 739.3 ms | 746.5 ms | 55.5 MiB | 56.8 MiB |
| 256 MiB guest / 1 vCPU | 31 | 744.6 ms | 752.9 ms | 58.2 MiB | 59.5 MiB |

The earlier successful Run 04 used the narrower launch-through-output-and-VMM-exit
boundary: medians 743.2/750.9 ms for 128/256 MiB, p95 755.0/759.4 ms. Run 05's
same narrow measurements were 738.5/743.7 ms. Both hosts ran 31 randomized
complete blocks, each containing all three profiles. The process control is not
equivalent isolation, so its lower latency is not a backend recommendation.

Halving configured guest RAM did **not** halve measured memory: the final host's
peak cgroup charges differed by about 2.7 MiB. PSS and cgroup charge measure
different accounting sets; neither includes the whole host's shared kernel,
artifact/page cache, controller or future renderer cost. Host available-memory
observations are also retained. There is no concurrency/density result here.

[Recomputed analysis](analysis.json), [Run 04 journal](run-04/results/firecracker/samples.jsonl),
[Run 05 journal](run-05/results/firecracker/samples.jsonl),
[Run 04 summary](run-04/results/firecracker/summary.json) and
[Run 05 summary](run-05/results/firecracker/summary.json) retain all 186 successful
records. Median/p95 were independently recomputed from those records, blocks
were checked for balance, and all 124 VMM status records checked. There were no
recorded cgroup OOM kills. Observation uses 5 ms polling; these are instrumented
host timings, not precise client-input/frame-ready latency. No statistical
speedup or competitor-confidence claim follows from this baseline.

## The boundary actually exercised

Both successful hosts were Ubuntu 24.04 `c8i.large`, 2 vCPUs / 4 GiB on Xeon
6975P-C, with `NestedVirtualization=enabled`. `/dev/kvm`, API version 12 and
KVM_CREATE_VM succeeded. Official Firecracker/jailer **v1.18.0** and Linux guest
**6.1.188** were hash checked against [pinned inputs](../../scripts/firecracker-inputs.json).
Static Sable was built with Rust 1.93.1 for `x86_64-unknown-linux-musl`.
Each fresh host also passed all **38 Rust tests**.

Every VMM ran under its matching jailer, unique UID/GID, chroot, new PID/mount
namespaces, default seccomp and an isolated network namespace. Actual sampled
state showed Seccomp=2, NoNewPrivs=1, zero effective capabilities and nested PID 1.
Run 05 also retains the host namespace baselines to verify the differences.
The sequential test samples share one isolated network namespace. Each VMM had
CPU quota 1, memory.max=512 MiB and pids.max=64. These caps are not its guest RAM
size. The guest jobs dropped to UID/GID 1000 and no-new-privileges.

The guest used only an initramfs and serial console: no block/network/vsock
device, MMDS, host filesystem share, remote Mac or renderer was configured.
A guest filesystem sentinel checks fresh boot state. Apple runtimes/SDKs and
Apple-linked probe binaries were not transferred to these hosts or guests.
Only original freestanding ARM64 fixtures were used. The graphics inventory
records no exposed render node; it does not prove graphics compatibility.

Artifacts include [final inputs](run-05/results/firecracker/inputs.json),
[all final VM/control logs](run-05/results/firecracker),
[Run 04 exact driver source](run-04/driver-source.txt), and
[Run 05 exact driver source](run-05/driver-source.txt). Archived driver hashes
match the input receipts. The [current harness](../../scripts/firecracker-bench.py)
is the final tested driver. The initial RAM filesystem includes third-party
Linux/BusyBox components locally; binaries are not published in this repository.

## Failed attempts are retained

Three earlier attempts did not count as completed benchmark blocks:

1. [Run 01](run-01/results/firecracker/000-vm128.log): Firecracker rejected a
   missing required `drives` list. The config now provides explicit empty
   block/network lists.
2. [Run 02](run-02/results/firecracker/000-vm128.log): the supervisor treated the
   jailer launcher's exit as VMM completion. The jailer intentionally spawns a
   new PID namespace/session and exits. The supervisor now tracks its owned VMM
   PID and captures failure state before cleanup.
3. [Run 03](run-03/results/firecracker/000-vm128.log): the guest completed Sable's
   work, but `poweroff` halted Linux and left the VMM resident. The 30-second
   deadline rejected the lifecycle. Its [failure state](run-03/results/firecracker/000-vm128-failure.json)
   records the still-live contained VMM. Guest reboot through the configured
   reset mechanism now produces a verified clean Firecracker exit.

All five outer runs verified zero owned AWS instances, volumes, security groups
and EC2 key pairs after cleanup. Each inner run verified no remaining owned
cgroup, jail or network namespace. See [Run 04 cleanup](run-04/cleanup.json),
[Run 05 cleanup](run-05/cleanup.json), their inner cleanup receipts, and the
[outer intervals](outer-timings.json). Recovery state/private SSH keys remain
outside Git.

## Cost and next gates

AWS Pricing quoted [USD 0.09371/hour](host-rate.json) for Linux On-Demand C8i.large
in N. Virginia on the observation date. This is compute only, not an invoice or
complete workflow cost. The outer intervals include provisioning, downloads,
builds and verified cleanup; per-job timings exclude that shared setup.
Run 04 excludes per-job pre-launch file preparation and post-exit resource
cleanup from its narrow clock. Run 05 retains that clock plus a complete-job
clock including those operations. Source/instrumentation differences are
preserved rather than merging them into one latency distribution.

Next compare warm workers and snapshot/restore with full preparation, reset,
identity and teardown costs counted. Then test concurrency and agent-wait gaps
after the actual app/graphics path works. A paused/halted VM retains RAM, as the
failed teardown demonstrated. This result proves neither GPU-free iPhone app
execution nor 2–4× better completed app workflows per dollar than Limrun.
See the [sandbox design](../../docs/sandbox-design.md) and
[app correctness/cost protocol](../../docs/app-benchmarks.md).
