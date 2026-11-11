# 150 concurrent Sable CPU microVMs

Observed November 10, 2026 (Pacific). On one fresh EC2 C8i.2xlarge, the pinned
experiment completed **six 150-guest bursts**: three with 128 MiB guests, then
three with 64 MiB guests. All requested guests were simultaneously ready and
resident before work release. Every guest completed 20 fully checked CPU
workloads and exited cleanly. These are Sable CPU sessions, **not iPhone apps**.

The [native app video](../recordings/README.md) separately shows actual app
interactions on Apple's Simulator. Sable still rejects that app at the
unsupplied UIKit dependency and completes zero UI interactions.

## Measured capacity and latency

One host: 8 vCPUs, configured 16 GiB, Ubuntu 24.04, host Linux 7.0.0-1014-aws,
nested KVM enabled, no swap. Firecracker/jailer v1.18.0 and guest Linux 6.1.188
use the same pinned inputs as the [sequential experiment](../sandbox/RESULTS.md).
All four hosts that reached the benchmark stage first passed 38 Rust tests;
the separate provisioning failure did not run those tests.
The Sable executable SHA-256 remains
`3ececfcee08f570ca9a11d1b238b09dee14fbc672f7181b87ebd52dc3a456765`.

| Measurement at 150 simultaneous guests | 128 MiB guest profile | 64 MiB guest profile |
| --- | ---: | ---: |
| Complete successful bursts | 3/3 | 3/3 |
| Median time from wave-request event until all-ready observation | 9.482 s | 9.598 s |
| Median time from work-release event until all-correct observation | 12.097 s | 12.561 s |
| Worst observed per-wave guest p95 after release | 12.376 s | 12.842 s |
| Median full wave through verified teardown | 26.392 s | 27.038 s |
| Maximum sampled parent cgroup charge | 8.118 GiB | 8.055 GiB |
| Minimum observed host available memory | 6.477 GiB | 6.504 GiB |

Each workload bundle runs the two initializer fixtures plus the 200,000-round
chained C fixture, checking both text outputs and the binary checksum. Thus
each 150-guest burst verifies 3,000 bundles / 9,000 Sable invocations. It is a
burst of bounded CPU work, not a sustained app or renderer workload. Three
bursts per profile do not establish a production capacity or a tail-latency SLO.

Halving configured guest RAM saved only about 65 MiB in the two profiles'
maximum aggregate charge, under 1%. It did **not** double density. Nor did the
64 MiB profile improve this run's active-work latency. The profiles ran in a
fixed order, not randomized A/B blocks; there is no significant speedup claim.

During the explicit three-second ready hold, median aggregate VMM cgroup CPU
usage was about 0.206 host CPU cores for 128 MiB and 0.168 for 64 MiB. These
numbers **exclude the controller and other host work**. The controller polls
logs, procfs and cgroups, and itself consumes resources. VMM PSS, parent charge
and host available memory have different accounting boundaries. The raw
[analysis](analysis.json) retains them separately; no number is a complete
future app/renderer/service footprint.

## Real failures and the placement experiment

The unpinned scheduler runs failed at higher load:

- [Light-work run](scale-128/run-01/results/firecracker): waves at 1, 4, 16, 32
  and 64 all passed. The 96-guest wave failed before work release. Eleven VMM
  logs reported `KVM_EXIT_FAIL_ENTRY` reason 7 on host CPU 5 within milliseconds.
- [Repeated-work run](repeated-work/run-01/results/firecracker-128): 1, 16 and
  64 guests passed with 20 checked bundles per guest. The 150-guest wave then
  failed with VM-entry errors across several host CPUs. Because the harness
  stopped on failure, that run never reached its 64 MiB profile.
- [First pinned provisioning attempt](pinned/run-01/controller.log): the
  initial DescribeInstances read returned CLI status 254 before a benchmark
  ran. Its underlying stderr was not captured, so the cause is undetermined.
  Cleanup verified zero resources. The controller now logs and retries that
  idempotent read within its existing deadline; it never repeats RunInstances.

The final experiment assigns each VMM one host CPU in round-robin order using
an inherited affinity mask. The host still has only eight vCPUs. All twelve
waves (1, 16, 64, 150, 150, 150 for each RAM profile) passed, including 1,062
fresh guests. Observed VMM leader affinity matched the assignment. Kernel VMCS
dump diagnostics were enabled on this disposable host. This is a promising
placement result, **not proof of the cause or a universal nested-KVM fix**:
the failed and successful placement runs used different EC2 hosts. A same-host
alternating control is still needed.

Reason 7 is Linux's `VMXERR_ENTRY_INVALID_CONTROL_FIELD`; KVM returns its
VM-instruction error through `KVM_EXIT_FAIL_ENTRY`. That identifies the failure
category, not the particular bad field or responsible layer. See the pinned
[VMX error definition](https://github.com/torvalds/linux/blob/v7.0/arch/x86/include/asm/vmx.h)
and [KVM exit path](https://github.com/torvalds/linux/blob/v7.0/arch/x86/kvm/vmx/vmx.c#L6417-L6431).
The captured [kernel event stream](repeated-work/host-kernel-stream.log) reports
the VMCS diagnostic setting. Firecracker's
[tested host-kernel matrix](https://github.com/firecracker-microvm/firecracker/blob/v1.18.0/docs/kernel-policy.md)
lists 5.10, 6.1 and 6.18, not this 7.0 host. That is another controlled variable
to investigate, not evidence that the kernel version caused these failures.

## What the proof records

Every guest gets a separate UID/GID, jail, PID/mount/network namespaces and
cgroup. The controller waits for a fully published owned VMM PID and checks
cgroup membership, UID/GID, zero effective capabilities, seccomp, no-new-privileges
and namespace differences. It records one readiness event per guest and holds
all guests resident for three seconds before sending `RUN` through their serial
input pipes. After every guest checks all bundles, they wait for a second
`EXIT` release and reboot into a verified clean Firecracker exit.

The per-guest release writes occur in a controller loop, not at an exactly
simultaneous hardware instant. Published work latency begins at the recorded
work-release event, after those writes finish. Earlier guests can begin before
that event. Observation uses a 25 ms polling sleep plus the time to inspect
every guest; it is not a
precise app-input/frame latency measurement. Full-wave time includes per-job
namespace/jail preparation, the explicit three- and one-second holds, boot,
work, shutdown and cgroup/jail/namespace cleanup. Shared downloads, build and
initramfs construction are excluded. These guests have no block/network/vsock
device, MMDS, host filesystem share, Apple runtime or renderer.

The original per-guest fields timestamp the start of their polling pass, before
log/procfs inspection. At 150 guests that places aggregate readiness/work
timestamps roughly 60–90 ms before the corresponding phase events. The table
and analysis use the emitted observer events; original poll-pass fields remain
unchanged in the raw receipts and are retained separately in the analysis.

[Re-audit script](../../scripts/density-report.py) independently checks every
passing guest log, workload count, namespace/privilege record, zero OOM count,
archived driver hash and cleanup receipt. Across all density attempts, it
audits **1,265 completed guest sessions / 22,982 checked workload bundles**;
failed waves are retained separately. [CPU proof replay](cpu-proof.cast) is an
asciicast v2 generated from the actual timestamped events of one audited
150-guest burst. It replays events, not an iPhone screen. Use `asciinema play`
to view it; the source events and their hash are in its header.

The exact final [driver](pinned/driver-source.txt),
[controller](pinned/controller-source-run-02.txt),
[128 MiB receipts](pinned/run-02/results/firecracker-128),
[64 MiB receipts](pinned/run-02/results/firecracker-64),
[inner cleanup](pinned/run-02/results/firecracker-64/cleanup.json) and
[AWS cleanup](pinned/run-02/cleanup.json) are retained. All five new provisioning
attempts verified zero remaining owned instances, volumes, groups and keys.
No Apple executable/framework/SDK was uploaded. Only the native recording and
curated original-app receipts are published, not the app build or xcresult.

## Resource economics and next contracts

The AWS Pricing response quoted [USD 0.37484/hour](host-rate.json) for this
Linux On-Demand host in N. Virginia. That is compute only, not an invoice,
per-app cost or proof that 150 heavy sessions fit. CPU and memory admission must
consider correlated active work and fully dirtied guest RAM, not just the
observed idle footprint of these trusted fixtures. The per-VMM CPU quota is a
cap, not a reserved core; 150 guest vCPUs oversubscribe eight host vCPUs.

Next isolate the VM-entry failure with same-host affinity controls and a
supported host kernel; profile boot/kernel and translation costs before tuning
them; then add longer resident/continuous-load tests and pool-level admission.
Snapshot/worker reuse must prove fresh writable state and identity. Real UIKit,
graphics, streaming and external app input remain prerequisites for claiming
iPhone sandbox density or better completed workflows per dollar than Limrun.
