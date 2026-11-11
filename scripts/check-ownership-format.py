#!/usr/bin/env python3
"""Inspect the original ARC/ownership matrix without running any guest code."""
import argparse
import json
from pathlib import Path
import runpy

inspect = runpy.run_path(str(Path(__file__).with_name('check-string-format.py')))['inspect']
p = argparse.ArgumentParser(description=__doc__)
p.add_argument('format', choices=['absolute', 'compact'])
p.add_argument('directory', type=Path)
o = p.parse_args()
print(json.dumps([inspect(o.directory / n, o.format, ownership=True) for n in [
    'client.macho', 'observer.macho', 'Foundation.dylib', 'CoreFoundation.dylib'
]], indent=2))
