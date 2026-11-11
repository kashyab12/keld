#!/usr/bin/env python3
"""Inspect original exact Q-ivar/void-setter layout and fixup representations."""
import argparse,json,runpy
from pathlib import Path
inspect=runpy.run_path(str(Path(__file__).with_name('check-super-format.py')))['inspect']
p=argparse.ArgumentParser(description=__doc__);p.add_argument('format',choices=['absolute','compact']);p.add_argument('directory',type=Path);p.add_argument('profile',type=int,choices=range(4));p.add_argument('--split',action='store_true');o=p.parse_args()
rs,cs=[(16,24),(8,16),(64,64),(16,64)][o.profile]
files=[o.directory/f'payload-{n}.macho' for n in [0,1]]
if o.split:files.append(o.directory/'libsable_base.dylib')
print(json.dumps([inspect(f,o.format,o.split,('PayloadBase','PayloadChild',None),False,(rs,cs)) for f in files],indent=2))
