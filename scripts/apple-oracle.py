#!/usr/bin/env python3
"""Run original fixtures through a temporary, privately owned CoreSimulator set.

This checks Apple's loader/ARM64 path locally; it never sends Apple software to Linux.
Requires a Mac with full Xcode and an available iOS runtime. No global selection changes.
"""
import json
import os
from pathlib import Path
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parent.parent
env = dict(os.environ, DEVELOPER_DIR=os.environ.get("DEVELOPER_DIR", "/Applications/Xcode.app/Contents/Developer"))


def sim(*args, owned_set=None, timeout=60, check=True):
    prefix = ["xcrun", "simctl"]
    if owned_set:
        prefix += ["--set", str(owned_set)]
    return subprocess.run(prefix + list(args), env=env, capture_output=True, text=True, timeout=timeout, check=check)


def main():
    runtimes = json.loads(sim("list", "runtimes", "--json").stdout)["runtimes"]
    runtime = next(r for r in runtimes if r["isAvailable"] and r.get("platform") == "iOS" and any(t["identifier"] == "com.apple.CoreSimulator.SimDeviceType.iPhone-17" for t in r["supportedDeviceTypes"]))
    base = Path(os.environ.get("MAKO_THREAD_DATA_DIR", tempfile.gettempdir()))
    evidence = ROOT / "evidence/apple-oracle.json"
    receipt = {"runtime": runtime["identifier"], "version": runtime["version"], "build": runtime["buildversion"], "runs": []}
    owned_set = Path(tempfile.mkdtemp(prefix="sable-oracle-", dir=base))
    device = None
    try:
        device = sim("create", "Sable oracle", "com.apple.CoreSimulator.SimDeviceType.iPhone-17", runtime["identifier"], owned_set=owned_set).stdout.strip()
        sim("boot", device, owned_set=owned_set)
        sim("bootstatus", device, "-b", owned_set=owned_set, timeout=180)
        for name in ["hello", "compute"]:
            result = sim("spawn", device, str(ROOT / "modules/sable/fixtures/bin" / (name + ".macho")), owned_set=owned_set, check=False)
            receipt["runs"].append({"fixture": name, "exit": result.returncode, "stdout": result.stdout, "stderr": result.stderr})
        receipt["passed"] = all(r["exit"] == 0 for r in receipt["runs"]) and receipt["runs"][0]["stdout"] == "SABLE_ARM64_DARWIN_OK\n"
    finally:
        if device:
            sim("shutdown", device, owned_set=owned_set, check=False)
            deleted = sim("delete", device, owned_set=owned_set, check=False)
            remaining = json.loads(sim("list", "devices", "--json", owned_set=owned_set).stdout)["devices"]
            receipt["cleanup"] = {"delete_exit": deleted.returncode, "remaining_devices": sum(len(v) for v in remaining.values())}
        evidence.parent.mkdir(exist_ok=True)
        evidence.write_text(json.dumps(receipt, indent=2) + "\n")
    print(json.dumps(receipt, indent=2))
    if not receipt.get("passed") or receipt.get("cleanup", {}).get("remaining_devices") != 0:
        raise SystemExit(1)


if __name__ == "__main__":
    main()
