#!/usr/bin/env python3
"""Read host graphics exposure; absence of a tool is never proof of no GPU.

Does not install drivers, load modules, create a display or start a remote worker.
Run on the benchmark host, not on the controller that launched it.
"""
import argparse
import datetime
import glob
import json
import os
from pathlib import Path
import platform
import shutil
import subprocess


def inventory():
    pci = Path("/sys/bus/pci/devices")
    devices = []
    errors = []
    if pci.is_dir():
        for directory in sorted(pci.iterdir()):
            try:
                device_class = (directory / "class").read_text().strip()
                if int(device_class, 16) >> 16 == 3:
                    driver = directory / "driver"
                    devices.append(dict(pci_address=directory.name, pci_class=device_class,
                                        vendor=(directory / "vendor").read_text().strip(), device=(directory / "device").read_text().strip(),
                                        driver=driver.resolve().name if driver.exists() else None))
            except OSError as error:
                errors.append(dict(device=directory.name, error=type(error).__name__))
    nodes = sorted(glob.glob("/dev/dri/renderD*") + glob.glob("/dev/nvidia[0-9]*") + glob.glob("/dev/dxg"))
    commands = {}
    for tool, args in [("vulkaninfo", ["--summary"]), ("nvidia-smi", ["--query-gpu=name,driver_version,memory.total", "--format=csv,noheader"] )]:
        if not shutil.which(tool):
            commands[tool] = dict(status="not_installed")
            continue
        try:
            result = subprocess.run([tool, *args], capture_output=True, text=True, timeout=15)
            commands[tool] = dict(status="ran", exit=result.returncode, stdout=result.stdout, stderr=result.stderr)
        except subprocess.TimeoutExpired:
            commands[tool] = dict(status="timed_out")
    return dict(schema=1, captured_at=datetime.datetime.now(datetime.timezone.utc).isoformat(),
                host=dict(os=platform.system(), kernel=platform.release(), arch=platform.machine()),
                pci_inventory_available=pci.is_dir(), pci_graphics=devices, pci_errors=errors, exposed_gpu_nodes=nodes,
                graphics_commands=commands,
                software_renderer_overrides={k: os.environ[k] for k in ["LIBGL_ALWAYS_SOFTWARE", "MESA_LOADER_DRIVER_OVERRIDE", "GALLIUM_DRIVER"] if k in os.environ},
                display_environment_present={k: bool(os.environ.get(k)) for k in ["DISPLAY", "WAYLAND_DISPLAY"]},
                interpretation="Exposure inventory only. Headless does not mean GPU-free. A software Vulkan device is not a hardware GPU. App rendering is a separate correctness gate.")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path)
    args = parser.parse_args()
    result = inventory()
    encoded = json.dumps(result, indent=2) + "\n"
    if args.output:
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(encoded)
    print(encoded, end="")


if __name__ == "__main__":
    main()
