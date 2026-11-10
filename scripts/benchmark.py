#!/usr/bin/env python3
"""Balanced, randomized process-level Sable/QEMU comparison on Linux.

Inputs are our original Mach-O fixtures and computationally identical repackaged
ELF files. Includes process startup/loading/translation for every engine. All
samples must exit correctly with identical output. No product benchmark claim.
"""
import argparse
import hashlib
import json
import math
from pathlib import Path
import platform
import random
import statistics
import subprocess
import time


def percentile(samples, fraction):
    return sorted(samples)[max(0,math.ceil(len(samples)*fraction)-1)]


def ratio_interval(reference, candidate, rng):
    ratios=[]
    for _ in range(5000):
        chosen=[rng.randrange(len(reference)) for _ in reference]
        ratios.append(statistics.median(reference[i] for i in chosen)/statistics.median(candidate[i] for i in chosen))
    return dict(ratio_of_medians=statistics.median(reference)/statistics.median(candidate),paired_bootstrap_95_percent=[percentile(ratios,.025),percentile(ratios,.975)])


def main():
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('--pairs',type=int,default=31);p.add_argument('--seed',type=int,default=12648430);p.add_argument('--output',type=Path,default=Path('artifacts/benchmark.json'));o=p.parse_args()
    if o.pairs<30:p.error('at least 30 randomized complete blocks required')
    o.output.parent.mkdir(parents=True,exist_ok=True);journal=o.output.with_suffix('.jsonl')
    if journal.exists():raise RuntimeError('refusing to overwrite an existing sample journal: '+str(journal))
    rng=random.Random(o.seed);records=[];summaries={}
    commands={engine:['target/release/sable','run','PLACEHOLDER',engine] for engine in ['interp','jit']}
    commands['qemu']=['qemu-aarch64','PLACEHOLDER']
    fixtures={}
    def sample(fixture,engine,block):
        path=Path('modules/sable/fixtures/bin')/(fixture+'.macho') if engine!='qemu' else Path('artifacts')/(fixture+'.elf')
        cmd=[str(path) if v=='PLACEHOLDER' else v for v in commands[engine]]
        started=time.perf_counter_ns();r=subprocess.run(['/usr/bin/time','-f','SABLE_TIME %U %S %M',*cmd],capture_output=True,timeout=30);elapsed=(time.perf_counter_ns()-started)/1e9
        metrics=next((line.split()[1:] for line in r.stderr.decode().splitlines() if line.startswith('SABLE_TIME ')),None)
        if not metrics:raise RuntimeError('missing GNU time metrics: '+r.stderr.decode())
        correct=r.returncode==0 and r.stdout==fixtures[fixture]['expected']
        record=dict(fixture=fixture,engine=engine,block=block,wall_seconds=elapsed,user_seconds=float(metrics[0]),system_seconds=float(metrics[1]),max_rss_kib=int(metrics[2]),returncode=r.returncode,output_sha256=hashlib.sha256(r.stdout).hexdigest(),correct=correct)
        if block>=0:
            with journal.open('a') as f:f.write(json.dumps(record)+'\n')
            records.append(record)
        if not correct:raise RuntimeError('incorrect sample: '+json.dumps(record)+'\n'+r.stderr.decode())
    for fixture,expected in [('checksum',(50_000_005_000_000).to_bytes(8,'little')),('workload',(0x8e65b49356b423fa).to_bytes(8,'little'))]:
        fixtures[fixture]=dict(expected=expected)
        for engine in commands:
            for _ in range(2):sample(fixture,engine,-1)
        for block in range(o.pairs):
            order=list(commands);rng.shuffle(order)
            for engine in order:sample(fixture,engine,block)
        timings={engine:[r['wall_seconds'] for r in records if r['fixture']==fixture and r['engine']==engine] for engine in commands}
        summaries[fixture]={engine:dict(median_seconds=statistics.median(v),p95_seconds=percentile(v,.95),min_seconds=min(v),max_seconds=max(v),median_max_rss_kib=statistics.median(r['max_rss_kib'] for r in records if r['fixture']==fixture and r['engine']==engine)) for engine,v in timings.items()}
        summaries[fixture]['interp_over_jit']=ratio_interval(timings['interp'],timings['jit'],rng)
        summaries[fixture]['qemu_over_jit']=ratio_interval(timings['qemu'],timings['jit'],rng)
    fingerprints={str(p):hashlib.sha256(p.read_bytes()).hexdigest() for p in [Path('target/release/sable'),*[Path('modules/sable/fixtures/bin')/(n+'.macho') for n in fixtures],*[Path('artifacts')/(n+'.elf') for n in fixtures]]}
    report=dict(protocol='process_startup_load_translate_execute',blocks=o.pairs,seed=o.seed,host=dict(system=platform.system(),machine=platform.machine(),release=platform.release()),qemu_version=subprocess.check_output(['qemu-aarch64','--version'],text=True).splitlines()[0],fingerprints=fingerprints,samples=len(records),failures=sum(not r['correct'] for r in records),summaries=summaries,limitations=['original freestanding fixtures, no Apple frameworks or UI','ELF adaptation changes only fixed syscall protocol instructions','process-level wall time includes startup, file loading and translation','confidence intervals resample paired blocks within one host; not fleet-wide estimates','GNU time CPU durations have 10 ms resolution','max RSS is per-process peak, not total host memory or achievable density'])
    o.output.write_text(json.dumps(report,indent=2)+'\n');print(json.dumps(report))


if __name__=='__main__':main()
