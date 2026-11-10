#!/usr/bin/env python3
"""Summarize paired worker benchmarks; initialization is accounted separately."""
import hashlib
import json
from pathlib import Path
import random
import statistics
import sys
from benchmark import ratio_interval, percentile

root=Path(sys.argv[1])
report={"protocol":"in_process_worker_link_compile_reset_execute","workloads":{},"limitations":["original programs and minimal LibSystem subset; no UIKit/Foundation/TLS","object bytes are already in host memory; no process or file-read startup included","prepared variants exclude one-time linking/translation; setup and amortization are reported","all variants allocate/reset private writable memory and stack","intervals are paired within one instance, not fleet-wide uncertainty"]}
for rounds in [8,2000,200000]:
    data=json.loads((root/f'dynamic-{rounds}.json').read_text());samples=data['samples_seconds'];pairs=data['pairs']
    assert data['full_state_equal'] and pairs>=30 and all(len(v)==pairs for v in samples)
    journal=[json.loads(l) for l in (root/f'dynamic-{rounds}.jsonl').read_text().splitlines()]
    assert len(journal)==pairs*4 and all(r['full_state_equal'] for r in journal)
    assert {(r['block'],r['variant']) for r in journal}=={(b,v) for b in range(pairs) for v in range(4)}
    rng=random.Random(12648430)
    metrics={name:{"median_seconds":statistics.median(v),"p95_seconds":percentile(v,.95)} for name,v in zip(data['variants'],samples)}
    metrics.update(cold_dispatch_speedup=ratio_interval(samples[0],samples[1],rng),prepared_dispatch_speedup=ratio_interval(samples[2],samples[3],rng),preparation_reuse_speedup=ratio_interval(samples[1],samples[3],rng),combined_speedup=ratio_interval(samples[0],samples[3],rng))
    metrics['one_observed_native_setup_seconds']=data['link_seconds']+data['native_prepare_seconds']
    metrics['amortized_prepared_native_seconds_per_run']=(data['link_seconds']+data['native_prepare_seconds']+sum(samples[3]))/pairs
    metrics['checksum_hex']=data['checksum_hex'];metrics['shared_image_bytes_per_run']=data['shared_image_bytes_per_run'];metrics['instructions']=data['instructions'];metrics['samples']=len(journal)
    report['workloads'][str(rounds)]=metrics
paths=[Path('target/release/sable'),Path('target/release/examples/dynamic_bench'),*sorted(Path('modules/sable/fixtures/bin/dynamic').glob('*'))]
report['fingerprints']={str(p):hashlib.sha256(p.read_bytes()).hexdigest() for p in paths}
print(json.dumps(report,indent=2))
