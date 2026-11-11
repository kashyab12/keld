#!/usr/bin/env python3
"""Run original fixtures through a temporary, privately owned CoreSimulator set.

This checks Apple's loader/ARM64 path locally; it never sends Apple software to Linux.
Requires a Mac with full Xcode and an available iOS runtime. No global selection changes.
"""
import argparse
import json
import os
from pathlib import Path
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parent.parent
env = dict(os.environ, DEVELOPER_DIR=os.environ.get("DEVELOPER_DIR", "/Applications/Xcode.app/Contents/Developer"))


def sim(*args, owned_set=None, timeout=60, check=True, text=True):
    prefix = ["xcrun", "simctl"]
    if owned_set:
        prefix += ["--set", str(owned_set)]
    return subprocess.run(prefix + list(args), env=env, capture_output=True, text=text, timeout=timeout, check=check)


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output",type=Path,default=ROOT / "evidence/apple-oracle.json")
    parser.add_argument("--dynamic",action="store_true")
    parser.add_argument("--chained",action="store_true")
    parser.add_argument("--initializers",action="store_true")
    parser.add_argument("--tls",action="store_true")
    parser.add_argument("--objc",action="store_true")
    parser.add_argument("--protocols",action="store_true")
    options=parser.parse_args()
    runtimes = json.loads(sim("list", "runtimes", "--json").stdout)["runtimes"]
    runtime = next(r for r in runtimes if r["isAvailable"] and r.get("platform") == "iOS" and any(t["identifier"] == "com.apple.CoreSimulator.SimDeviceType.iPhone-17" for t in r["supportedDeviceTypes"]))
    base = Path(os.environ.get("MAKO_THREAD_DATA_DIR", tempfile.gettempdir()))
    evidence = options.output
    receipt = {"runtime": runtime["identifier"], "version": runtime["version"], "build": runtime["buildversion"], "runs": []}
    owned_set = Path(tempfile.mkdtemp(prefix="sable-oracle-", dir=base))
    device = None
    try:
        device = sim("create", "Sable oracle", "com.apple.CoreSimulator.SimDeviceType.iPhone-17", runtime["identifier"], owned_set=owned_set).stdout.strip()
        sim("boot", device, owned_set=owned_set)
        sim("bootstatus", device, "-b", owned_set=owned_set, timeout=180)
        expected={"hello":b"SABLE_ARM64_DARWIN_OK\n", "compute":b"", "workload":(0x8e65b49356b423fa).to_bytes(8,"little"), "checksum":(50_000_005_000_000).to_bytes(8,"little")}
        if options.dynamic:expected.update({"dynamic/dynamic-8":(0x38afd5bab).to_bytes(8,"little"),"dynamic/dynamic-2000":(0xefe1edd5865163fb).to_bytes(8,"little"),"dynamic/dynamic-200000":(0xa7151818301809c9).to_bytes(8,"little")})
        if options.chained:expected.update({"chained/dynamic-8":(0x38afd5bab).to_bytes(8,"little"),"chained/dynamic-2000":(0xefe1edd5865163fb).to_bytes(8,"little"),"chained/dynamic-200000":(0xa7151818301809c9).to_bytes(8,"little"),"chained/chained-addend":(51).to_bytes(8,"little")})
        if options.initializers:expected.update({f"initializers/{mode}/initialize":b"INIT leaf 1\nINIT leaf 2\nINIT parent\nINIT sibling\nINIT executable\nARGS\nMAIN\n" for mode in ["offsets","pointers"]})
        if options.tls:expected.update({f"tls/{mode}/tls-{ctor}":b"TLV scalar ABI, templates, two images, stable addresses OK\n" for mode in ['chained','legacy'] for ctor in [0,1]})
        if options.objc:expected.update({f"{profile}/{mode}/objc-{ctor}":b"ObjC root class, scalar instance/class dispatch, nil and selectors OK\n" for profile in ['objc','objc-compact'] for mode in ['chained','legacy'] for ctor in [0,1]})
        if options.protocols:expected.update({f"protocols/{fmt}/{mode}/protocol-{ctor}":b"Protocol lookup, names, declared adoption and guest IMP OK\n" for fmt in ['absolute','compact'] for mode in ['chained','legacy'] for ctor in [0,1]})
        for name in expected:
            result = sim("spawn", device, str(ROOT / "modules/sable/fixtures/bin" / (name + ".macho")), owned_set=owned_set, check=False, text=False)
            receipt["runs"].append({"fixture": name, "exit": result.returncode, "stdout_hex": result.stdout.hex(), "stderr": result.stderr.decode(errors="replace")})
        receipt["passed"]=all(r["exit"]==0 and r["stdout_hex"]==expected[r["fixture"]].hex() for r in receipt["runs"])

    finally:
        if device:
            sim("shutdown", device, owned_set=owned_set, check=False)
            deleted = sim("delete", device, owned_set=owned_set, check=False)
            remaining = json.loads(sim("list", "devices", "--json", owned_set=owned_set).stdout)["devices"]
            receipt["cleanup"] = {"delete_exit": deleted.returncode, "remaining_devices": sum(len(v) for v in remaining.values())}
        evidence.parent.mkdir(parents=True, exist_ok=True)
        evidence.write_text(json.dumps(receipt, indent=2) + "\n")
    print(json.dumps(receipt, indent=2))
    if not receipt.get("passed") or receipt.get("cleanup", {}).get("remaining_devices") != 0:
        raise SystemExit(1)


if __name__ == "__main__":
    main()
