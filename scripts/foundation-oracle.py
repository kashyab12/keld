#!/usr/bin/env python3
"""Original local-only Foundation startup oracle; no tracked source writes or Apple binary export."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parent.parent
parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--output', required=True, type=Path, help='New private artifact directory')
options = parser.parse_args()
BASE = options.output.resolve()
if BASE.is_relative_to(ROOT) and not BASE.is_relative_to(ROOT / 'artifacts'):
    parser.error('use ignored artifacts/ or a private directory outside the repository')
BASE.mkdir(parents=True, exist_ok=False)
SRC = ROOT / 'modules/sable/fixtures/foundation'
OUT = BASE / 'results'
ENV = dict(os.environ, DEVELOPER_DIR='/Applications/Xcode.app/Contents/Developer')
OUT.mkdir(exist_ok=True)
receipt = {'runs': [], 'builds': [], 'cleanup': {}}

def run(args, *, env=ENV, timeout=60, check=True):
    p = subprocess.run(args, env=env, capture_output=True, text=True, timeout=timeout)
    if check and p.returncode:
        raise RuntimeError(f'{args[0]} failed {p.returncode}: {p.stderr}')
    return p

def sim(*args, owned=None, **kw):
    return run(['xcrun', 'simctl'] + (['--set', str(owned)] if owned else []) + list(args), **kw)

def build(platform, target):
    dest = OUT / platform
    dest.mkdir(exist_ok=True)
    sdk = run(['xcrun', '--sdk', platform, '--show-sdk-path']).stdout.strip()
    common = ['xcrun', 'clang', '-target', target, '-isysroot', sdk, '-O1', '-g', '-fno-stack-protector']
    leaf, parent = dest / 'libprobe_leaf.dylib', dest / 'libprobe_parent.dylib'
    commands = [common + ['-dynamiclib', str(SRC / 'leaf.c'), '-Wl,-install_name,@rpath/libprobe_leaf.dylib', '-o', str(leaf)],
                common + ['-dynamiclib', str(SRC / 'parent.c'), str(leaf), '-Wl,-install_name,@rpath/libprobe_parent.dylib', '-Wl,-rpath,@loader_path', '-o', str(parent)],
                common + ['-fobjc-arc', str(SRC / 'minimal.m'), '-framework', 'Foundation', '-o', str(dest / 'minimal')],
                common + ['-fobjc-arc', str(SRC / 'contracts.m'), str(parent), str(leaf), '-Wl,-rpath,@executable_path', '-framework', 'Foundation', '-o', str(dest / 'contracts')]]
    for cmd in commands:
        p = run(cmd)
        receipt['builds'].append({'platform': platform, 'exit': p.returncode, 'stderr': p.stderr})
    for name in ['minimal', 'contracts', leaf.name, parent.name]:
        binary = dest / name
        run(['codesign', '-s', '-', str(binary)])
        meta = run(['xcrun', 'dyld_info', '-linked_dylibs', '-imports', '-inits', '-segments', str(binary)])
        (dest / (name + '.metadata.txt')).write_text(meta.stdout + meta.stderr)
    return dest

def record(platform, name, p):
    (OUT / platform / (name + '.stdout.txt')).write_text(p.stdout)
    (OUT / platform / (name + '.stderr.txt')).write_text(p.stderr)
    receipt['runs'].append({'platform': platform, 'program': name, 'exit': p.returncode, 'stdout': p.stdout,
                            'stderr_lines': len(p.stderr.splitlines())})

owned = device = None
try:
    receipt['xcode'] = run(['xcodebuild', '-version']).stdout.strip()
    receipt['macos'] = run(['sw_vers']).stdout.strip()
    for platform, target in [('macosx', 'arm64-apple-macos14'), ('iphonesimulator', 'arm64-apple-ios17-simulator')]:
        dest = build(platform, target)
        if platform == 'macosx':
            for name in ['minimal', 'contracts']:
                record(platform, name, run([str(dest / name)], env=dict(ENV, DYLD_PRINT_INITIALIZERS='1'), check=False))
    runtimes = json.loads(sim('list', 'runtimes', '--json').stdout)['runtimes']
    runtime = next(r for r in runtimes if r['isAvailable'] and r.get('buildversion') == '24B8117')
    receipt['runtime'] = {k: runtime[k] for k in ['identifier', 'version', 'buildversion']}
    owned = Path(tempfile.mkdtemp(prefix='foundation-private-set-', dir=BASE))
    receipt['owned_set'] = str(owned)
    device = sim('create', 'Sable original Foundation oracle', 'com.apple.CoreSimulator.SimDeviceType.iPhone-17', runtime['identifier'], owned=owned).stdout.strip()
    receipt['owned_uuid'] = device
    (OUT / 'receipt.json').write_text(json.dumps(receipt, indent=2) + '\n')
    sim('boot', device, owned=owned)
    sim('bootstatus', device, '-b', owned=owned, timeout=180)
    dest = OUT / 'iphonesimulator'
    for name in ['minimal', 'contracts']:
        record('iphonesimulator', name, sim('spawn', device, str(dest / name), owned=owned,
               env=dict(ENV, SIMCTL_CHILD_DYLD_PRINT_INITIALIZERS='1'), check=False))
    receipt['passed'] = len(receipt['runs']) == 4 and all(r['exit'] == 0 and
        ('SABLE FOUNDATION\n' == r['stdout'] if r['program'] == 'minimal' else
         r['stdout'].startswith('INIT leaf\nINIT parent\nINIT executable\nMAIN\n') and 'FOUNDATION CONTRACTS OK\n' in r['stdout'])
        for r in receipt['runs'])
except Exception as e:
    receipt['error'] = str(e)
    receipt['passed'] = False
finally:
    if device:
        stopped = sim('shutdown', device, owned=owned, check=False)
        deleted = sim('delete', device, owned=owned, check=False)
        remaining = json.loads(sim('list', 'devices', '--json', owned=owned).stdout)['devices']
        receipt['cleanup'] = {'shutdown_exit': stopped.returncode, 'delete_exit': deleted.returncode,
            'remaining_devices': sum(len(v) for v in remaining.values()),
            'owned_uuid_present': any(d['udid'] == device for ds in remaining.values() for d in ds)}
    receipt['driver_sha256'] = hashlib.sha256(Path(__file__).read_bytes()).hexdigest()
    receipt['source_sha256'] = {p.name: hashlib.sha256(p.read_bytes()).hexdigest() for p in SRC.iterdir() if p.is_file()}
    (OUT / 'receipt.json').write_text(json.dumps(receipt, indent=2) + '\n')
print(json.dumps(receipt, indent=2))
raise SystemExit(0 if receipt.get('passed') and receipt.get('cleanup', {}).get('remaining_devices') == 0 else 1)
