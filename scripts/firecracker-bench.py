#!/usr/bin/env python3
"""Owned Linux/KVM research host only: jailed Firecracker runs original Sable jobs.

No Apple runtime, graphics, network device, block device, MMDS or cloud credential
enters a microVM. This is a sandbox overhead experiment, not an iOS app benchmark.
Requires root on the explicitly created disposable AWS host; never run on a shared
developer machine. AWS lifecycle/cleanup is controlled by aws-smoke.py --firecracker.
"""
import argparse
import fcntl
import hashlib
import json
import math
import os
from pathlib import Path
import platform
import random
import shutil
import signal
import stat
import statistics
import subprocess
import tarfile
import time
import urllib.request
import uuid

ROOT = Path(__file__).resolve().parent.parent


def command(args, **kwargs):
    return subprocess.run(args, capture_output=True, check=True, timeout=120, **kwargs).stdout


def save(path, value):
    path.write_text(json.dumps(value, indent=2) + "\n")


def download(url, expected, path):
    with urllib.request.urlopen(url, timeout=60) as source, path.open("wb") as target:
        shutil.copyfileobj(source, target)
    digest = hashlib.sha256(path.read_bytes()).hexdigest()
    if digest != expected:
        raise RuntimeError("download hash mismatch: " + path.name)
    return digest


def memory_available():
    return int(next(line.split()[1] for line in Path('/proc/meminfo').read_text().splitlines() if line.startswith('MemAvailable:'))) * 1024


def status(pid):
    path=Path('/proc')/str(pid)
    try:
        info=dict(line.split(':',1) for line in (path/'status').read_text().splitlines())
        fields={k:info.get(k,'').strip() for k in ['Uid','Gid','CapEff','NoNewPrivs','Seccomp','NSpid','VmRSS','Threads','State']}
        fields['namespaces']={k:os.readlink(path/'ns'/k) for k in ['pid','mnt','net']}
        fields['pss_kib']=int(next(line.split()[1] for line in (path/'smaps_rollup').read_text().splitlines() if line.startswith('Pss:')))
        return fields
    except (OSError,StopIteration):
        return None


def running(pid):
    try:
        state=next(line for line in (Path('/proc')/str(pid)/'status').read_text().splitlines() if line.startswith('State:'))
        return not state.split()[1] in ['Z','X']
    except (OSError,StopIteration):
        return False


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output',required=True,type=Path)
    parser.add_argument('--blocks',type=int,default=31)
    options=parser.parse_args()
    if os.geteuid()!=0 or platform.system()!='Linux' or platform.machine()!='x86_64':
        parser.error('requires root on the owned x86-64 Linux/KVM research instance')
    if options.blocks<1 or options.blocks>101: parser.error('blocks must be 1..101')
    base=(ROOT/'artifacts'/('firecracker-'+str(uuid.uuid4())[:8])).resolve()
    base.mkdir(parents=True,mode=0o755)
    output=options.output.resolve();output.mkdir(parents=True,exist_ok=False)
    run_id='sable-'+str(uuid.uuid4())[:12]
    recovery={'run_id':run_id,'base':str(base),'cgroup_parent':run_id,'owned_process_groups':[]}
    save(base/'recovery.json',recovery)
    kvm=os.open('/dev/kvm',os.O_RDWR|os.O_CLOEXEC)
    try:
        version=fcntl.ioctl(kvm,0xAE00,0)
        vm=fcntl.ioctl(kvm,0xAE01,0)
        os.close(vm)
    finally: os.close(kvm)
    if version!=12: raise RuntimeError('unexpected KVM API version')
    pins=json.loads((ROOT/'scripts/firecracker-inputs.json').read_text())
    download(pins['firecracker']['url'],pins['firecracker']['sha256'],base/'release.tgz')
    with tarfile.open(base/'release.tgz') as archive: archive.extractall(base,filter='data')
    version=pins['firecracker']['version']
    release=base/f'release-{version}-x86_64'
    firecracker=release/f'firecracker-{version}-x86_64'
    jailer=release/f'jailer-{version}-x86_64'
    download(pins['kernel']['url'],pins['kernel']['sha256'],base/'kernel')
    download(pins['kernel']['config_url'],pins['kernel']['config_sha256'],base/'kernel.config')
    if 'CONFIG_BLK_DEV_INITRD=y' not in (base/'kernel.config').read_text(): raise RuntimeError('kernel lacks initrd')
    payload=base/'payload';payload.mkdir(mode=0o755)
    shutil.copyfile(ROOT/'target/x86_64-unknown-linux-musl/release/sable',payload/'sable');(payload/'sable').chmod(0o755)
    shutil.copyfile('/usr/bin/busybox',payload/'busybox');(payload/'busybox').chmod(0o755)
    shutil.copytree(ROOT/'modules/sable/fixtures/bin',payload/'fixtures')
    shutil.copyfile(ROOT/'scripts/firecracker-workload.sh',payload/'workload.sh')
    shutil.copyfile(ROOT/'modules/sable/fixtures/init-expected.txt',payload/'expected-init.txt')
    (payload/'expected-checksum.bin').write_bytes((0xa7151818301809c9).to_bytes(8,'little'))
    command(['musl-gcc','-static','-O2',str(ROOT/'scripts/firecracker-drop.c'),'-o',str(base/'drop')])
    initrd=base/'initrd';initrd.mkdir()
    shutil.copytree(payload,initrd/'payload')
    shutil.copyfile(base/'drop',initrd/'drop');(initrd/'drop').chmod(0o755)
    for name in ['bin','dev','proc','sys','tmp']: (initrd/name).mkdir()
    os.mknod(initrd/'dev/console',stat.S_IFCHR|0o600,os.makedev(5,1))
    (initrd/'tmp').chmod(0o1777)
    shutil.copyfile('/usr/bin/busybox',initrd/'bin/busybox');(initrd/'bin/busybox').chmod(0o755)
    for name in ['sh','mount','reboot','cmp','mkdir','cat']: (initrd/'bin'/name).symlink_to('busybox')
    (initrd/'init').write_text('''#!/bin/sh
set -eu
mount -t proc proc /proc
mount -t sysfs sysfs /sys
mount -t devtmpfs devtmpfs /dev
if [ -e /session-state ]; then echo SABLE_RESET_FAILED; reboot -f; fi
echo private > /session-state
echo SABLE_VM_READY
if /drop /payload/busybox sh /payload/workload.sh /payload /tmp/work; then
  echo SABLE_VM_OK
else
  echo SABLE_VM_FAILED
fi
reboot -f
''');(initrd/'init').chmod(0o755)
    names=command(['find','.','-print0'],cwd=initrd)
    packed=command(['cpio','--null','-o','--format=newc','--quiet'],input=names,cwd=initrd)
    (base/'initrd.cpio').write_bytes(packed)
    (base/'kernel').chmod(0o444);(base/'initrd.cpio').chmod(0o444)
    info={'scope':'trusted original ARM64 fixtures in jailed Linux microVMs; no iOS UI',
          'driver_sha256':hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
          'payload_driver_sha256':hashlib.sha256((ROOT/'scripts/firecracker-workload.sh').read_bytes()).hexdigest(),
          'drop_source_sha256':hashlib.sha256((ROOT/'scripts/firecracker-drop.c').read_bytes()).hexdigest(),
          'host_kernel':platform.release(),'host_arch':platform.machine(),'kvm_api':12,
          'firecracker_version':command([str(firecracker),'--version']).decode().strip(),
          'jailer_version':command([str(jailer),'--version']).decode().strip(),
          'pins':pins,'sable_sha256':hashlib.sha256((payload/'sable').read_bytes()).hexdigest(),
          'initrd_sha256':hashlib.sha256(packed).hexdigest(),'initrd_bytes':len(packed),
          'busybox_sha256':hashlib.sha256((payload/'busybox').read_bytes()).hexdigest(),
          'uid_gid':1000,'guest_vcpus':1,'network_devices':0,'block_devices':0,'mmds_configured':False}
    save(output/'inputs.json',info)
    host_ns={name:os.readlink('/proc/self/ns/'+name) for name in ['pid','mnt','net']}
    info['host_namespaces']=host_ns;save(output/'inputs.json',info)
    # These changes are scoped to this disposable instance, not a shared host.
    cgroot=Path('/sys/fs/cgroup')
    with (cgroot/'cgroup.subtree_control').open('w') as f: f.write('+cpu +memory +pids')
    parent=cgroot/run_id;parent.mkdir()
    (parent/'cgroup.subtree_control').write_text('+cpu +memory +pids')
    records=[];rng=random.Random(0x5ab1e)
    netns=run_id
    command(['ip','netns','add',netns])
    journal=output/'samples.jsonl'
    try:
        for block in range(options.blocks):
            modes=['process','vm128','vm256'];rng.shuffle(modes)
            for mode in modes:
                seq=len(records);uid=20000+seq;identifier=f'job-{seq}'
                requested=time.perf_counter_ns()
                log=output/f'{seq:03}-{mode}.log';process=None;cgroup=None
                available_before=memory_available();minimum_available=available_before
                observed=None;peak_pss=0;ready_ns=None;correct_ns=None
                vmm_pid=None
                jail=base/'jails'/firecracker.name/identifier/'root'
                if mode=='process':
                    work=base/f'work-{seq}';work.mkdir();os.chown(work,1000,1000)
                    args=[str(base/'drop'),str(payload/'busybox'),'sh',str(payload/'workload.sh'),str(payload),str(work)]
                else:
                    jail.mkdir(parents=True)
                    os.link(base/'kernel',jail/'kernel');os.link(base/'initrd.cpio',jail/'initrd')
                    config={'boot-source':{'kernel_image_path':'/kernel','initrd_path':'/initrd','boot_args':'console=ttyS0 reboot=k panic=1 pci=off quiet loglevel=3 rdinit=/init'},'drives':[],'network-interfaces':[],
                            'machine-config':{'vcpu_count':1,'mem_size_mib':int(mode[2:]),'smt':False}}
                    save(jail/'vm.json',config)
                    args=[str(jailer),'--id',identifier,'--exec-file',str(firecracker),'--uid',str(uid),'--gid',str(uid),
                          '--new-pid-ns','--netns','/run/netns/'+netns,'--chroot-base-dir',str(base/'jails'),
                          '--cgroup-version','2','--parent-cgroup',run_id,
                          '--cgroup','memory.max=536870912','--cgroup','pids.max=64','--cgroup','cpu.max=100000 100000',
                          '--resource-limit','no-file=256','--','--no-api','--config-file','/vm.json']
                    cgroup=parent/identifier
                started=time.perf_counter_ns()
                try:
                    with log.open('wb') as stream:
                        process=subprocess.Popen(args,stdin=subprocess.DEVNULL,stdout=stream,stderr=subprocess.STDOUT,start_new_session=True)
                        recovery['owned_process_groups']=[process.pid];save(base/'recovery.json',recovery)
                        deadline=time.monotonic()+30
                        while True:
                            now=time.perf_counter_ns();data=log.read_bytes()
                            if b'SABLE_VM_READY' in data and ready_ns is None: ready_ns=now
                            marker=b'SABLE_WORK_OK' if mode=='process' else b'SABLE_VM_OK'
                            if marker in data and correct_ns is None: correct_ns=now
                            minimum_available=min(minimum_available,memory_available())
                            pidfile=jail/(firecracker.name+'.pid')
                            if mode!='process' and pidfile.exists():
                                if vmm_pid is None:
                                    vmm_pid=int(pidfile.read_text())
                                    recovery['owned_vmm_pid']=vmm_pid;save(base/'recovery.json',recovery)
                                candidate=status(vmm_pid)
                                if candidate and candidate['NoNewPrivs']=='1' and candidate['Seccomp']=='2' and int(candidate['Uid'].split()[0])==uid:
                                    observed=candidate;peak_pss=max(peak_pss,candidate['pss_kib'])
                            launcher_exit=process.poll()
                            if launcher_exit is not None:
                                if mode=='process': break
                                # The jailer intentionally exits after clone; its
                                # child also gets a new session. Follow that owned
                                # VMM, not the launcher's PID/process group.
                                if vmm_pid is not None and not running(vmm_pid): break
                                if vmm_pid is None: raise RuntimeError('jailer exited without an owned VMM pid')
                            if time.monotonic()>deadline: raise TimeoutError('owned job exceeded 30 seconds')
                            time.sleep(.005)
                    ended=time.perf_counter_ns();data=log.read_bytes()
                    marker=b'SABLE_WORK_OK' if mode=='process' else b'SABLE_VM_OK'
                    if marker not in data or b'SABLE_VM_FAILED' in data or b'SABLE_RESET_FAILED' in data: raise RuntimeError('guest/job validation failed: '+log.name)
                    if mode=='process' and process.returncode!=0: raise RuntimeError('process control failed after output')
                    if mode!='process':
                        if b'Firecracker exiting successfully. exit_code=0' not in data:
                            raise RuntimeError('VMM did not report clean exit: '+log.name)
                        if observed is None or observed['CapEff']!='0000000000000000' or any(observed['namespaces'][name]==host_ns[name] for name in host_ns):
                            raise RuntimeError('missing privilege/namespace evidence: '+log.name)
                    record={'block':block,'mode':mode,'elapsed_seconds':(ended-started)/1e9,
                            'observed_correct_seconds':((correct_ns or ended)-started)/1e9,
                            'observed_ready_seconds':(ready_ns-started)/1e9 if ready_ns else None,
                            'correct':True,'peak_sampled_pss_kib':peak_pss if observed else None,
                            'host_mem_available_before':available_before,'host_mem_available_min':minimum_available,
                            'vmm_status':observed,'launcher_exit':process.returncode}
                    if cgroup and cgroup.exists():
                        record['cgroup']={name:(cgroup/name).read_text().strip() for name in ['memory.max','memory.peak','memory.events','cpu.max','cpu.stat','pids.max','pids.events']}
                except Exception as error:
                    failure={'block':block,'mode':mode,'error':str(error),'launcher_exit':process.poll() if process else None,'vmm_pid':vmm_pid,'last_valid_status':observed}
                    if cgroup and cgroup.exists():
                        failure['cgroup']={name:(cgroup/name).read_text().strip() for name in ['cgroup.procs','memory.peak','memory.events','cpu.stat']}
                    save(output/f'{seq:03}-{mode}-failure.json',failure)
                    raise
                finally:
                    if process and process.poll() is None:
                        try: os.killpg(process.pid,signal.SIGTERM)
                        except ProcessLookupError: pass
                        try: process.wait(timeout=3)
                        except subprocess.TimeoutExpired:
                            os.killpg(process.pid,signal.SIGKILL);process.wait(timeout=3)
                    if cgroup and cgroup.exists():
                        procs=(cgroup/'cgroup.procs').read_text().strip()
                        if procs:
                            (cgroup/'cgroup.kill').write_text('1')
                            for _ in range(100):
                                if not (cgroup/'cgroup.procs').read_text().strip():break
                                time.sleep(.01)
                        cgroup.rmdir()
                    if jail.parent.exists():shutil.rmtree(jail.parent)
                    recovery['owned_process_groups']=[];save(base/'recovery.json',recovery)
                record['full_job_seconds']=(time.perf_counter_ns()-requested)/1e9
                records.append(record)
                with journal.open('a') as f: f.write(json.dumps(record)+'\n')
        summaries={}
        for mode in ['process','vm128','vm256']:
            selected=[r for r in records if r['mode']==mode];times=sorted(r['elapsed_seconds'] for r in selected)
            summaries[mode]={'samples':len(times),'median_seconds':statistics.median(times),'p95_seconds':times[math.ceil(.95*len(times))-1],
                             'median_full_job_seconds':statistics.median(r['full_job_seconds'] for r in selected),
                             'p95_full_job_seconds':sorted(r['full_job_seconds'] for r in selected)[math.ceil(.95*len(times))-1],
                             'max_cgroup_peak_bytes':max((int(r['cgroup']['memory.peak']) for r in selected if 'cgroup' in r),default=None)}
        save(output/'summary.json',{'blocks':options.blocks,'measure':'cold lifecycle through validated output and VMM exit; process control has no matched isolation',
                                   'all_correct':True,'summaries':summaries,'app_workflows':0,'competitive_speedup_claim':False})
    finally:
        command(['ip','netns','delete',netns])
        if parent.exists():parent.rmdir()
        cleanup={'remaining_owned_cgroups':len(list(cgroot.glob(run_id+'/*'))),'owned_netns_remaining':Path('/run/netns',netns).exists(),'remaining_owned_jails':len(list((base/'jails').glob('*/*'))) if (base/'jails').exists() else 0}
        save(output/'cleanup.json',cleanup)
    print((output/'summary.json').read_text())


if __name__=='__main__':main()
