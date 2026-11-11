#!/usr/bin/env python3
"""Inspect original class graphs and exact scalar object-pointer signatures."""
import argparse
import json
from pathlib import Path
import runpy
inspect=runpy.run_path(str(Path(__file__).with_name('check-super-format.py')))['inspect']
p=argparse.ArgumentParser(description=__doc__);p.add_argument('format',choices=['absolute','compact']);p.add_argument('directory',type=Path);p.add_argument('--split',action='store_true');o=p.parse_args()
files=[o.directory/f'instance-{n}.macho' for n in [0,1]]
if o.split:files.append(o.directory/'libsable_base.dylib')
print(json.dumps([inspect(f,o.format,o.split,('AllocBase','AllocChild',None),True) for f in files],indent=2))
