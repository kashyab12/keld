#!/usr/bin/env python3
"""Audit recorded density waves and emit a table plus an actual-event terminal replay."""
import argparse
import hashlib
import json
import math
from pathlib import Path
import statistics


def audit(root):
    rows=[];failures=[];candidates=[]
    for path in sorted(root.rglob('wave-[0-9][0-9].json')):
        result=json.loads(path.read_text());folder=path.parent
        inputs=json.loads((folder/'inputs.json').read_text())
        archive=next((base/'driver-source.txt' for base in folder.parents if (base/'driver-source.txt').exists() and base.is_relative_to(root)),None)
        assert archive and hashlib.sha256(archive.read_bytes()).hexdigest()==inputs['driver_sha256'], path
        count=result['requested_concurrency'];metrics=result['metrics'];jobs=result['jobs']
        assert result['all_correct'] and len(jobs)==count, path
        assert metrics['max_simultaneously_ready']==count and metrics['max_simultaneously_work_complete']==count, path
        assert metrics['ready_hold_seconds']>=3, path
        repeats=inputs.get('work_repetitions',1)
        assert len({j['uid'] for j in jobs})==count, path
        for ns in ['pid','mnt','net']:
            assert len({j['status']['namespaces'][ns] for j in jobs})==count, (path,ns)
        for slot,job in enumerate(jobs):
            status=job['status']
            assert status['NoNewPrivs']=='1' and status['Seccomp']=='2' and status['CapEff']=='0000000000000000', (path,slot)
            assert all(int(n)==job['uid'] for n in status['Uid'].split()+status['Gid'].split()), (path,slot)
            assert len(status['NSpid'].split())==2 and status['NSpid'].split()[-1]=='1', (path,slot)
            assert all(status['namespaces'][k]!=inputs['host_namespaces'][k] for k in ['pid','mnt','net']), (path,slot)
            assert int(dict(line.split() for line in job['cgroup']['memory.events'].splitlines())['oom_kill'])==0, (path,slot)
            if job.get('pinned_cpu') is not None:assert status['Cpus_allowed_list']==str(job['pinned_cpu']), (path,slot)
            data=(folder/f"wave-{result['wave']:02}-{slot:03}.log").read_bytes()
            assert data.count(b'SABLE_WORK_OK')==repeats and b'SABLE_VM_OK' in data, (path,slot)
            assert b'Firecracker exiting successfully. exit_code=0' in data, (path,slot)
            assert not any(marker in data for marker in [b'SABLE_VM_FAILED',b'SABLE_RESET_FAILED',b'SABLE_PROTOCOL_FAILED']), (path,slot)
        events=[json.loads(line) for line in (folder/'events.jsonl').read_text().splitlines()
                if json.loads(line).get('wave')==result['wave']]
        phase={e['event']:e for e in events}
        completed=[e for e in events if e['event']=='work_correct_waiting']
        assert len(completed)==count and {e['id'] for e in completed}=={j['id'] for j in jobs},path
        assert phase['all_ready_resident']['count']==count and phase['all_work_correct_resident']['count']==count,path
        release=phase['work_released']['monotonic_ns']
        # Per-guest raw fields use poll-pass start, before procfs/log inspection.
        # Use the actual emitted observer events for the published latency clock.
        times=sorted((e['monotonic_ns']-release)/1e9 for e in completed)
        ready_seconds=(phase['all_ready_resident']['monotonic_ns']-phase['wave_requested']['monotonic_ns'])/1e9
        work_seconds=(phase['all_work_correct_resident']['monotonic_ns']-release)/1e9
        assert times[0]>=0, path
        cpu=lambda key: int(dict(line.split() for line in metrics[key].splitlines())['usage_usec'])
        row=dict(path=str(path.relative_to(root)),concurrency=count,guest_mib=result['guest_mib'],repetitions=repeats,
                 pin_policy=inputs.get('pin_policy','host_scheduler'),all_ready_seconds=ready_seconds,
                 work_p50_seconds=statistics.median(times),work_p95_seconds=times[math.ceil(.95*len(times))-1],
                 all_work_seconds=work_seconds,full_wave_seconds=result['full_wave_seconds'],
                 raw_poll_pass_all_ready_seconds=metrics['all_ready_seconds'],
                 raw_poll_pass_all_work_seconds=metrics['all_work_correct_seconds'],
                 peak_parent_charge_bytes=metrics['peak_parent_current_bytes'],
                 peak_sampled_aggregate_vmm_pss_kib=metrics['peak_sampled_aggregate_vmm_pss_kib'],
                 host_available_min_bytes=metrics['min_host_available_bytes'],
                 idle_vmm_cpu_cores=(cpu('idle_cpu_after')-cpu('idle_cpu_before'))/1e6/metrics['ready_hold_seconds'],
                 checked_workloads_per_active_second=count*repeats/work_seconds)
        rows.append(row);candidates.append((row,path,result))
    for path in sorted(root.rglob('*failure.json')):
        failures.append(dict(path=str(path.relative_to(root)),**json.loads(path.read_text())))
    for path in sorted(root.rglob('cleanup.json')):
        receipt=json.loads(path.read_text());assert not any(receipt.values()), path
    assert rows, 'no audited passing wave'
    report=dict(scope='trusted Sable CPU fixtures; no iPhone app workflow',
                latency_clock='emitted phase observer events, work release after dispatch writes; raw poll-pass fields preserved',
                waves=rows,failures=failures,
                checked_guest_sessions=sum(r['concurrency'] for r in rows),
                checked_workloads=sum(r['concurrency']*r['repetitions'] for r in rows),
                max_verified_concurrency=max(r['concurrency'] for r in rows),
                app_workflows=0,stable_production_capacity_established=False,competitive_speedup_claim=False)
    (root/'analysis.json').write_text(json.dumps(report,indent=2)+'\n')
    row,path,result=max(candidates,key=lambda item:(item[0]['concurrency'],item[0]['repetitions'],item[0]['pin_policy']))
    source=path.parent/'events.jsonl'
    events=[json.loads(line) for line in source.read_text().splitlines() if json.loads(line).get('wave')==result['wave']]
    begin=events[0]['monotonic_ns'];end=events[-1]['monotonic_ns'];ready=0;correct=0
    header=dict(version=2,width=112,height=30,title='Recorded Sable CPU microVM events, not an iPhone UI',
                duration=(end-begin)/1e9,env={'TERM':'xterm-256color'},
                source=str(source.relative_to(root)),source_sha256=hashlib.sha256(source.read_bytes()).hexdigest())
    with (root/'cpu-proof.cast').open('w') as stream:
        stream.write(json.dumps(header)+'\n')
        stream.write(json.dumps([0,'o',f"Sable CPU microVM proof: {row['concurrency']} guests, {row['repetitions']} checked workloads per guest. NOT an iPhone app UI.\r\n"] )+'\n')
        for event in events:
            if event['event']=='ready':ready+=1
            if event['event']=='work_correct_waiting':correct+=1
            elapsed=(event['monotonic_ns']-begin)/1e9
            text=f"{elapsed:8.3f}s  {event['event']:<27} {event.get('id',event.get('slot',''))!s:<16} ready={ready}/{row['concurrency']} correct={correct}/{row['concurrency']}\r\n"
            stream.write(json.dumps([elapsed,'o',text])+'\n')
    print(json.dumps({k:v for k,v in report.items() if k not in ['waves','failures']},indent=2))


if __name__=='__main__':
    parser=argparse.ArgumentParser(description=__doc__);parser.add_argument('--root',type=Path,default=Path('evidence/density'))
    audit(parser.parse_args().root)
