# Sandbox containment and efficiency

The first containment candidate is a small Linux microVM running Sable. Sable
provides ARM64/Darwin execution; Firecracker provides the VM boundary. App
compatibility, UIKit, graphics and input remain separate
[correctness gates](app-benchmarks.md).

The first [two-host execution baseline](../evidence/sandbox/RESULTS.md) completed
124 fresh VM jobs with verified results and teardown. It measures current
CPU/runtime fixtures. The [concurrency follow-up](../evidence/density/RESULTS.md)
completed six 150-guest CPU bursts on one 8-vCPU/16-GiB host with explicit CPU
placement, after retaining higher-load nested-KVM failures without pinning.
Application performance and application density remain unproved.

## The current experiment

AWS exposes nested virtualization on supported virtual instances, including
C8i. Enable it explicitly in CpuOptions, then verify `/dev/kvm` and KVM_CREATE_VM
on the actual host. AWS recommends evaluating bare metal for strict performance
requirements; nested measurements are a small test environment, not a bare-metal
claim. [AWS documentation](https://docs.aws.amazon.com/AWSEC2/latest/UserGuide/amazon-ec2-nested-virtualization.html).

The harness uses official Firecracker/jailer v1.18.0, pinned release/kernel hashes,
a static Sable executable and a small initramfs. It loads only original fixtures.
Each job gets fresh guest RAM/rootfs, checks initializer output and a chained C
checksum, and exits. No Apple runtime or SDK enters the guest.

The VMM uses the matching jailer with a chroot, changed UID/GID, PID/network
namespaces, default seccomp, zero effective capabilities and no-new-privileges. Each VMM gets
a separate cgroup CPU quota, memory cap and PID limit. The guest workload drops
to UID/GID 1000 with no-new-privileges. The configuration includes no network or
block device, MMDS, vsock, host filesystem share or remote renderer. An isolated
network namespace is shared across sequential samples. The gated concurrency
experiment instead gives each guest a distinct empty network namespace and
verifies the recorded namespace identities. Neither test exercises guest
network traffic or establishes an escape audit.
[Jailer contract](https://github.com/firecracker-microvm/firecracker/blob/v1.18.0/docs/jailer.md).

The outer AWS controller creates one tagged host, no instance role, IMDSv2,
caller-IP-only SSH and a terminate timer. It verifies removal of its instance,
volumes, security group and key. MicroVM cleanup targets only owned process
groups, cgroups, namespace and jail paths. Recovery state and SSH private keys
stay outside Git.

## Measurements and the next hill climbs

The harness randomizes complete blocks of an unisolated process control and
128/256 MiB, one-vCPU microVMs. The process control gives a minimum-overhead
reference, not equivalent isolation. Record launch through verified output and
VMM exit, readiness observation, sampled VMM PSS, cgroup memory/events/CPU use
and host available memory. Setup/download/build time is outside each sample but
inside the outer run duration. Keep failed attempts and partial journals.

The concurrency experiment gates all guests on serial input, observes three
seconds of simultaneous readiness, then releases repeated checked CPU work.
Round-robin host CPU affinity passed six 150-guest bursts; unpinned experiments
failed at 96/150 guests with KVM VM-entry errors. Different hosts were used, so
a same-host placement control remains necessary. The 64/128 MiB comparison
saved less than 1% of measured aggregate charge at 150 guests. Memory labels
are not physical working-set measurements or admission guarantees. The
8.1-GiB parent cgroup result excludes parts of the host and the future renderer.

These are trusted CPU/runtime jobs. After UIKit correctness, compare complete
app workflows across suitable isolation backends at concurrency 1/2/4/8 and
beyond, including continuous UI, bursts and agent waits. Count kernel/VMM,
runtime, renderer/encoder, controller, page cache, storage, network and failures.
Report success rate, throughput, p50/p95 latency, OOMs, total host memory and
VRAM. Pin app/runtime, viewport, driver and hardware across competitors.

The planned optimizations need complete-workflow evidence:

- Reuse translated immutable code and read-only artifacts while resetting
  writable state; measure total host memory rather than only shared image bytes.
- Compare cold boot, warm workers and snapshot restore. A paused microVM retains
  its memory; snapshot/teardown/restore has to earn its full cost.
- Freeze templates before session data, inject fresh identity/entropy, and verify
  clone/reset sentinels. Snapshots must not retain another session's files or TLS.
- Keep renderer/device ownership and external service trust explicit. A headless
  CPU test does not demonstrate a usable software graphics backend.

Firecracker's [design](https://github.com/firecracker-microvm/firecracker/blob/v1.18.0/docs/design.md)
describes its Linux/device scope and pause behavior. It does not supply an iPhone
graphics stack. The product score remains successful correct app workflows per
total dollar at matched latency; no boot/CPU result establishes a 2–4× Limrun
advantage. These research tools remain MIT. The future service module's intended
ELv2 scope remains in [licensing](../LICENSING.md).
