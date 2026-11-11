#!/usr/bin/env python3
"""One owned, disposable x86 EC2 host. AWS CLI credentials stay on this machine.

python3 scripts/aws-smoke.py run
python3 scripts/aws-smoke.py cleanup /path/to/private/state.json
"""
import argparse
import datetime
import hashlib
import io
import json
import os
from pathlib import Path
import shlex
import subprocess
import tarfile
import time
import urllib.request
import uuid

ROOT = Path(__file__).resolve().parent.parent
REGION = os.environ.get("AWS_REGION", "us-east-1")


def command(args, **kwargs):
    return subprocess.run(args, check=True, capture_output=True, **kwargs).stdout


def aws(*args):
    return json.loads(command(["aws", "--region", REGION, *args, "--output", "json"]))


def save(path, state):
    path.write_text(json.dumps(state, indent=2) + "\n")


def cleanup(path):
    state = json.loads(path.read_text())
    global REGION
    REGION = state["region"]
    # Recover a launch whose response may have been lost; never replay RunInstances.
    hosts = aws("ec2", "describe-instances", "--filters", "Name=client-token,Values=" + state["token"], "Name=instance-state-name,Values=pending,running,stopping,stopped")
    ids = [i["InstanceId"] for r in hosts["Reservations"] for i in r["Instances"]]
    if ids:
        aws("ec2", "terminate-instances", "--instance-ids", *ids)
        command(["aws", "--region", REGION, "ec2", "wait", "instance-terminated", "--instance-ids", *ids])
    if state.get("key_import_attempted"):
        keys = aws("ec2", "describe-key-pairs", "--filters", "Name=key-name,Values=" + state["name"])["KeyPairs"]
        if keys:
            aws("ec2", "delete-key-pair", "--key-name", state["name"])
    groups = aws("ec2", "describe-security-groups", "--filters", "Name=group-name,Values=" + state["name"])["SecurityGroups"]
    for group in groups:
        for attempt in range(12):
            try:
                aws("ec2", "delete-security-group", "--group-id", group["GroupId"])
                break
            except subprocess.CalledProcessError as error:
                if b"DependencyViolation" not in error.stderr or attempt == 11:
                    raise
                time.sleep(5)
    active = aws("ec2", "describe-instances", "--filters", "Name=client-token,Values=" + state["token"], "Name=instance-state-name,Values=pending,running,stopping,stopped")
    volumes = aws("ec2", "describe-volumes", "--filters", "Name=tag:KeldRun,Values=" + state["token"])["Volumes"]
    remaining_groups = aws("ec2", "describe-security-groups", "--filters", "Name=group-name,Values=" + state["name"])["SecurityGroups"]
    remaining_keys = aws("ec2", "describe-key-pairs", "--filters", "Name=key-name,Values=" + state["name"])["KeyPairs"]
    receipt = {"active_instances": sum(len(r["Instances"]) for r in active["Reservations"]), "volumes": len(volumes), "security_groups": len(remaining_groups), "key_pairs": len(remaining_keys)}
    if any(receipt.values()):
        raise RuntimeError("cleanup incomplete: " + json.dumps(receipt))
    state["cleanup"] = receipt
    state["cleaned_at"] = datetime.datetime.now(datetime.timezone.utc).isoformat()
    save(path, state)
    print("Cleanup verified:", json.dumps(receipt), flush=True)
    return receipt


def run(output=None, extended=False, dynamic=False, firecracker=False, concurrency=None, guest_mib=128, memory_sweep=False, work_repetitions=1, pin_cpus=False):
    instance_type = "c8i.2xlarge" if concurrency else ("c8i.large" if firecracker else "c7i.large")
    token = str(uuid.uuid4())
    name = "keld-sable-" + token[:12]
    base = Path(os.environ.get("MAKO_THREAD_DATA_DIR", str(Path.home() / ".cache/keld-sable")))
    private = base / name
    private.mkdir(parents=True, mode=0o700)
    path = private / "state.json"
    state = {"token": token, "name": name, "region": REGION, "started_at": datetime.datetime.now(datetime.timezone.utc).isoformat()}
    save(path, state)
    evidence = output or ROOT / "evidence/aws-smoke"
    if output and evidence.exists():
        raise RuntimeError("refusing to overwrite previous experiment: " + str(evidence))
    evidence.mkdir(parents=True, exist_ok=True)
    print("Private recovery state:", path, flush=True)
    try:
        vpc = aws("ec2", "describe-vpcs", "--filters", "Name=is-default,Values=true")["Vpcs"][0]["VpcId"]
        subnets = aws("ec2", "describe-subnets", "--filters", "Name=vpc-id,Values=" + vpc, "Name=default-for-az,Values=true")["Subnets"]
        subnet = next(s["SubnetId"] for s in subnets if s["MapPublicIpOnLaunch"])
        images = aws("ec2", "describe-images", "--owners", "099720109477", "--filters", "Name=name,Values=ubuntu/images/hvm-ssd-gp3/ubuntu-noble-24.04-amd64-server-*", "Name=state,Values=available")["Images"]
        image = max(images, key=lambda i: i["CreationDate"])
        group = aws("ec2", "create-security-group", "--group-name", name, "--description", "Disposable Sable translation test", "--vpc-id", vpc)["GroupId"]
        state["group"] = group
        save(path, state)
        ip = urllib.request.urlopen("https://checkip.amazonaws.com", timeout=15).read().decode().strip()
        import ipaddress
        ipaddress.IPv4Address(ip)
        aws("ec2", "authorize-security-group-ingress", "--group-id", group, "--protocol", "tcp", "--port", "22", "--cidr", ip + "/32")
        key = private / "ssh-key"
        command(["ssh-keygen", "-q", "-t", "ed25519", "-N", "", "-f", str(key)])
        state["key_import_attempted"] = True
        save(path, state)
        aws("ec2", "import-key-pair", "--key-name", name, "--public-key-material", "fileb://" + str(key) + ".pub")
        user_data = private / "user-data.sh"
        bootstrap = """#!/bin/bash
set -euxo pipefail
shutdown -h +45
export DEBIAN_FRONTEND=noninteractive
apt-get update -qq
apt-get install -y -qq build-essential clang curl ca-certificates python3-venv qemu-user time
curl --proto '=https' --tlsv1.2 -fsS https://sh.rustup.rs -o /tmp/sable-rustup.sh
sudo -iu ubuntu sh /tmp/sable-rustup.sh -y --profile minimal --default-toolchain 1.93.1 --component clippy,rustfmt
touch /tmp/sable-ready
"""
        if firecracker:
            bootstrap = bootstrap.replace("qemu-user time", "qemu-user time musl-tools busybox-static cpio iproute2")
            bootstrap = bootstrap.replace("touch /tmp/sable-ready", "sudo -iu ubuntu /home/ubuntu/.cargo/bin/rustup target add x86_64-unknown-linux-musl\nmodprobe kvm_intel\nif [ -f /sys/module/kvm_intel/parameters/dump_invalid_vmcs ]; then echo 1 > /sys/module/kvm_intel/parameters/dump_invalid_vmcs; fi\ntouch /tmp/sable-ready")
        user_data.write_text(bootstrap)
        tags = json.dumps([{"ResourceType": kind, "Tags": [{"Key": "Name", "Value": name}, {"Key": "KeldRun", "Value": token}]} for kind in ["instance", "volume"]])
        cpu_options = ["--cpu-options", "NestedVirtualization=enabled"] if firecracker else []
        instance = aws("ec2", "run-instances", *cpu_options, "--client-token", token, "--image-id", image["ImageId"], "--instance-type", instance_type, "--count", "1", "--subnet-id", subnet, "--security-group-ids", group, "--key-name", name, "--instance-initiated-shutdown-behavior", "terminate", "--metadata-options", "HttpTokens=required", "--block-device-mappings", json.dumps([{"DeviceName": image["RootDeviceName"], "Ebs": {"VolumeSize": 12, "VolumeType": "gp3", "DeleteOnTermination": True}}]), "--tag-specifications", tags, "--user-data", "file://" + str(user_data))["Instances"][0]
        state["instance"] = instance["InstanceId"]
        save(path, state)
        print(f"Launched owned {instance_type}; 45-minute terminate timer; SSH limited to caller /32.", flush=True)
        ssh_base = ["ssh", "-i", str(key), "-o", "StrictHostKeyChecking=accept-new", "-o", "UserKnownHostsFile=" + str(private / "known_hosts"), "-o", "ConnectTimeout=5"]
        deadline = time.monotonic() + 720
        while True:
            if time.monotonic() > deadline:
                raise TimeoutError("host bootstrap exceeded 12 minutes")
            # Retry only this idempotent read within the bootstrap deadline.
            # Never repeat RunInstances after an uncertain response.
            try:
                current = aws("ec2", "describe-instances", "--instance-ids", state["instance"])["Reservations"][0]["Instances"][0]
            except subprocess.CalledProcessError as error:
                with (evidence / "bootstrap-read-errors.jsonl").open('a') as log:
                    log.write(json.dumps({'operation':'DescribeInstances','exit':error.returncode,
                                          'stderr':error.stderr.decode(errors='replace')})+'\n')
                time.sleep(5)
                continue
            address = current.get("PublicIpAddress")
            if address:
                ready = subprocess.run(ssh_base + ["ubuntu@" + address, "test -f /tmp/sable-ready"], capture_output=True)
                if ready.returncode == 0:
                    break
            time.sleep(5)
        ssh = ssh_base + ["ubuntu@" + address]
        payload = io.BytesIO()
        runtime_paths = [ROOT / "Cargo.toml", ROOT / "Cargo.lock", *sorted((ROOT / "modules/sable").rglob("*"))]
        manifest = {str(p.relative_to(ROOT)): hashlib.sha256(p.read_bytes()).hexdigest() for p in runtime_paths if p.is_file()}
        (evidence / "runtime-source-sha256.json").write_text(json.dumps(manifest, indent=2) + "\n")
        with tarfile.open(fileobj=payload, mode="w:gz") as archive:
            for rel in ["Cargo.toml", "Cargo.lock", "modules", "scripts"]:
                archive.add(ROOT / rel, arcname=rel, filter=lambda info: None if "__pycache__" in info.name else info)
        command(ssh + ["mkdir -p sable && tar -xz -C sable"], input=payload.getvalue())
        remote = """set -e
export PATH="$HOME/.cargo/bin:$PATH"
cd sable
mkdir -p results
python3 scripts/graphics-inventory.py --output results/graphics-inventory.json
uname -sm
lscpu | head -30
rustc -V
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test --release
bash scripts/check-initializers.sh
bash scripts/check-tls.sh
bash scripts/check-objc.sh
for mode in chained legacy; do
  mkdir -p "results/objc/$mode"
  cp artifacts/objc/"$mode"/interp-*.txt artifacts/objc/"$mode"/jit-*.txt artifacts/objc/"$mode"/*-unknown.stderr "results/objc/$mode/"
done
for mode in chained legacy; do
  mkdir -p "results/tls/$mode"
  cp artifacts/tls/"$mode"/interp-*.txt artifacts/tls/"$mode"/jit-*.txt "results/tls/$mode/"
done
mkdir -p results/initializers
for mode in offsets pointers; do
  cp "artifacts/initializers/$mode/interp.txt" "results/initializers/$mode-interp.txt"
  cp "artifacts/initializers/$mode/jit.txt" "results/initializers/$mode-jit.txt"
done
bash scripts/check-chained.sh
mkdir -p results/chained
cp artifacts/chained/interp-*.bin artifacts/chained/jit-*.bin results/chained/
python3 -c 'import hashlib,json; from pathlib import Path; print(json.dumps({p.name:hashlib.sha256(p.read_bytes()).hexdigest() for p in sorted(Path("artifacts/chained").iterdir()) if p.suffix in [".macho",".dylib"]},indent=2))' > results/chained/rebuilt-sha256.json
bash scripts/build-fixtures.sh
cmp artifacts/hello.macho modules/sable/fixtures/bin/hello.macho
cmp artifacts/compute.macho modules/sable/fixtures/bin/compute.macho
cargo run --release -- run artifacts/hello.macho jit
cargo run --release -- run artifacts/compute.macho jit
cargo run --release -- bench artifacts/compute.macho 9
"""
        if extended:
            remote = remote[:remote.index('cargo run --release -- bench')] + """
cargo build --release --example oracle_batch
python3 -m venv oracle-venv
oracle-venv/bin/pip -q install unicorn==2.1.4
mkdir -p results
for seed in 12648430 42 8675309; do
  oracle-venv/bin/python scripts/oracle.py --engine interp --seed "$seed" --output "results/oracle-interp-$seed.json"
  oracle-venv/bin/python scripts/oracle.py --engine jit --seed "$seed" --output "results/oracle-jit-$seed.json"
done
# Validate the version rebuilt with this Linux host's Clang, too.
target/release/sable run artifacts/workload.macho jit > results/rebuilt-checksum.bin
python3 -c 'from pathlib import Path; assert Path("results/rebuilt-checksum.bin").read_bytes() == (0x8e65b49356b423fa).to_bytes(8,"little")'
python3 scripts/macho-to-elf.py modules/sable/fixtures/bin/checksum.macho artifacts/checksum.elf
python3 scripts/macho-to-elf.py modules/sable/fixtures/bin/workload.macho artifacts/workload.elf
taskset -c 0 python3 scripts/benchmark.py --pairs 31 --output results/benchmark.json
"""
        if dynamic:
            remote += """
cargo build --release --example dynamic_bench
bash scripts/build-dynamic.sh
for rounds in 8 2000 200000; do
  target/release/sable run "artifacts/dynamic/dynamic-$rounds.macho" jit --lib artifacts/dynamic/libsable_math.dylib --lib artifacts/dynamic/libsable_helper.dylib --lib artifacts/dynamic/libsable_system.dylib > "results/rebuilt-dynamic-$rounds.bin"
  taskset -c 0 target/release/examples/dynamic_bench "$rounds" 31 "results/dynamic-$rounds.jsonl" > "results/dynamic-$rounds.json"
done
python3 -c 'from pathlib import Path; expected={8:0x38afd5bab,2000:0xefe1edd5865163fb,200000:0xa7151818301809c9}; assert all(Path(f"results/rebuilt-dynamic-{n}.bin").read_bytes()==v.to_bytes(8,"little") for n,v in expected.items())'
python3 scripts/summarize-dynamic.py results > results/dynamic-summary.json
cat results/dynamic-summary.json
"""
        if firecracker:
            remote = """set -e
export PATH="$HOME/.cargo/bin:$PATH"
cd sable
mkdir -p results
python3 scripts/graphics-inventory.py --output results/graphics-inventory.json
uname -sm
lscpu | head -30
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test --release
cargo build --release --target x86_64-unknown-linux-musl
sudo python3 scripts/firecracker-bench.py --output results/firecracker --blocks 31
"""
            if concurrency:
                levels = ' '.join(str(n) for n in concurrency)
                remote = remote.replace('--blocks 31', '--concurrency '+levels+' --guest-mib '+str(guest_mib))
                remote = remote.replace('--guest-mib '+str(guest_mib), '--guest-mib '+str(guest_mib)+' --work-repetitions '+str(work_repetitions))
                if pin_cpus:remote=remote.replace('--work-repetitions '+str(work_repetitions),'--work-repetitions '+str(work_repetitions)+' --pin-cpus')
                if memory_sweep:
                    original=remote.splitlines()[-1]
                    remote=remote.replace(original,original.replace('results/firecracker','results/firecracker-128').replace('--guest-mib '+str(guest_mib),'--guest-mib 128')+'\n'+
                                          original.replace('results/firecracker','results/firecracker-64').replace('--guest-mib '+str(guest_mib),'--guest-mib 64'))
        started = time.monotonic()
        result = subprocess.run(ssh + ["bash -s"], input=remote.encode(), capture_output=True, timeout=720)
        (evidence / "stdout.txt").write_bytes(result.stdout)
        (evidence / "stderr.txt").write_bytes(result.stderr)
        public = {"region": REGION, "instance_type": instance_type, "cpu_options": current.get("CpuOptions", {}), "image_name": image["Name"], "started_at": state["started_at"], "remote_elapsed_seconds": time.monotonic() - started, "remote_exit": result.returncode}
        (evidence / "run.json").write_text(json.dumps(public, indent=2) + "\n")
        # Every mode records graphics exposure, even if a subsequent check fails.
        fetched = subprocess.run(ssh + ["tar -cz -C sable results"], capture_output=True)
        if fetched.returncode != 0:
            (evidence / "results-fetch.stderr.txt").write_bytes(fetched.stderr)
            raise RuntimeError("could not retrieve remote results; do not count this run as verified")
        # Entries come from our own directory; still reject links/path escapes.
        with tarfile.open(fileobj=io.BytesIO(fetched.stdout), mode="r:gz") as archive:
            archive.extractall(evidence, filter="data")
        if not (evidence / "results/graphics-inventory.json").is_file():
            raise RuntimeError("remote graphics inventory missing; do not count this run as verified")
        print(result.stdout.decode(), flush=True)
        if result.returncode:
            print(result.stderr.decode(), flush=True)
            raise RuntimeError("remote checks failed; logs preserved")
    finally:
        receipt = cleanup(path)
        (evidence / "cleanup.json").write_text(json.dumps(receipt, indent=2) + "\n")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("operation", choices=["run", "cleanup"])
    parser.add_argument("state", nargs="?", type=Path)
    parser.add_argument("--output", type=Path)
    parser.add_argument("--extended", action="store_true")
    parser.add_argument("--dynamic", action="store_true")
    parser.add_argument("--firecracker", action="store_true", help="owned C8i nested-KVM Firecracker experiment")
    parser.add_argument("--concurrency", type=int, nargs='+', help="Separate gated density experiment on one owned 8-vCPU/16-GiB host")
    parser.add_argument("--guest-mib", type=int, choices=[64,128,256], default=128)
    parser.add_argument("--memory-sweep", action='store_true', help="Compare 128 then 64 MiB guests on the same owned density host")
    parser.add_argument("--work-repetitions", type=int, default=1)
    parser.add_argument("--pin-cpus", action='store_true')
    options = parser.parse_args()
    if options.operation == "run":
        if options.concurrency and (not options.firecracker or any(n<1 or n>150 for n in options.concurrency)):
            parser.error('--concurrency requires --firecracker and levels 1..150')
        if (options.memory_sweep or options.work_repetitions!=1 or options.pin_cpus) and not options.concurrency:
            parser.error('memory sweep and repeated work require --concurrency')
        if not 1<=options.work_repetitions<=100:parser.error('work repetitions must be 1..100')
        if options.firecracker and (options.extended or options.dynamic): parser.error("--firecracker is a separate experiment")
        run(options.output, options.extended or options.dynamic, options.dynamic, options.firecracker, options.concurrency, options.guest_mib, options.memory_sweep, options.work_repetitions, options.pin_cpus)
    elif options.state:
        cleanup(options.state)
    else:
        parser.error("cleanup requires private state.json path")
