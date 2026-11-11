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
        fields={k:info.get(k,'').strip() for k in ['Uid','Gid','CapEff','NoNewPrivs','Seccomp','NSpid','VmRSS','Threads','State','Cpus_allowed_list']}
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


def published_pid(launcher, path):
    """The jailer creates its pidfile before writing it; exit 0 publishes it."""
    code = launcher.poll()
    if code is None:
        return None
    if code != 0:
        raise RuntimeError('jailer launcher failed: ' + str(code))
    value = path.read_text().strip()
    if not value.isascii() or not value.isdecimal() or int(value) <= 1:
        raise RuntimeError('invalid completed jailer pidfile')
    return int(value)


def cleanup_density_jobs(jobs):
    """Visit every owned resource even if another cleanup action fails."""
    errors=[]
    def attempt(job, resource, action):
        try:action()
        except Exception as error:
            errors.append(dict(job=job['id'],resource=resource,error=type(error).__name__+': '+str(error)))
    for job in jobs:
        def stop_cgroup():
            cg=job['cgroup']
            if not cg.exists():return
            if (cg/'cgroup.procs').read_text().strip():(cg/'cgroup.kill').write_text('1')
            for _ in range(300):
                if not (cg/'cgroup.procs').read_text().strip():break
                time.sleep(.01)
            cg.rmdir()
        attempt(job,'cgroup',stop_cgroup)
        proc=job.get('process')
        if proc:
            def stop_launcher():
                if proc.poll() is None:
                    try:os.killpg(proc.pid,signal.SIGKILL)
                    except ProcessLookupError:pass
                proc.wait(timeout=3)
            attempt(job,'launcher',stop_launcher)
            def close_input():
                if proc.stdin:
                    try:proc.stdin.close()
                    except BrokenPipeError:pass  # Guest exit can leave a failed buffered release.
            attempt(job,'serial_input',close_input)
        def remove_jail():
            if job['jail'].parent.exists():shutil.rmtree(job['jail'].parent)
        attempt(job,'jail',remove_jail)
        if job['namespace_created']:
            attempt(job,'network_namespace',lambda:command(['ip','netns','delete',job['namespace']]))
    return errors


def density(base, output, firecracker, jailer, parent, host_ns, levels, guest_mib, repetitions, pin_cpus):
    """Serial input gates prove simultaneous residency, then release real work."""
    events = (output/'events.jsonl').open('a')
    waves = []
    available_cpus=sorted(os.sched_getaffinity(0))
    def event(kind, **fields):
        events.write(json.dumps(dict(event=kind, monotonic_ns=time.perf_counter_ns(), **fields))+'\n'); events.flush()
    try:
        for wave, count in enumerate(levels):
            jobs=[]; requested=time.perf_counter_ns(); baseline=memory_available()
            metrics={'min_host_available_bytes':baseline, 'peak_parent_current_bytes':0,
                     'peak_sampled_aggregate_vmm_pss_kib':0, 'max_simultaneously_ready':0,
                     'max_simultaneously_work_complete':0}
            before_cpu=Path('/proc/stat').read_text().splitlines()[0]
            event('wave_requested', wave=wave, concurrency=count)
            try:
                for slot in range(count):
                    identifier=f'density-{wave}-{slot}'; uid=20000+wave*200+slot
                    jail=base/'jails'/firecracker.name/identifier/'root';jail.mkdir(parents=True)
                    os.link(base/'kernel',jail/'kernel');os.link(base/'initrd.cpio',jail/'initrd')
                    save(jail/'vm.json', {'boot-source':{'kernel_image_path':'/kernel','initrd_path':'/initrd',
                         'boot_args':'console=ttyS0 reboot=k panic=1 pci=off quiet loglevel=3 rdinit=/init'},
                         'drives':[], 'network-interfaces':[],
                         'machine-config':{'vcpu_count':1,'mem_size_mib':guest_mib,'smt':False}})
                    # A distinct empty network namespace for each concurrent job.
                    namespace=parent.name+'-'+str(slot)
                    job=dict(id=identifier,uid=uid,jail=jail,cgroup=parent/identifier,namespace=namespace,
                             namespace_created=False,log=output/f'wave-{wave:02}-{slot:03}.log',pid=None,ready=None,correct=None,status=None)
                    jobs.append(job)
                    command(['ip','netns','add',namespace]);job['namespace_created']=True
                    args=[str(jailer),'--id',identifier,'--exec-file',str(firecracker),'--uid',str(uid),'--gid',str(uid),
                          '--new-pid-ns','--netns','/run/netns/'+namespace,'--chroot-base-dir',str(base/'jails'),
                          '--cgroup-version','2','--parent-cgroup',parent.name,
                          '--cgroup','memory.max=268435456','--cgroup','pids.max=64','--cgroup','cpu.max=100000 100000',
                          '--resource-limit','no-file=256','--','--no-api','--config-file','/vm.json']
                    job['pinned_cpu']=available_cpus[slot%len(available_cpus)] if pin_cpus else None
                    if pin_cpus:args=['taskset','--cpu-list',str(job['pinned_cpu']),*args]
                    with job['log'].open('wb') as stream:
                        job['started']=time.perf_counter_ns()
                        job['process']=subprocess.Popen(args,stdin=subprocess.PIPE,stdout=stream,stderr=subprocess.STDOUT,start_new_session=True)
                    event('launched',wave=wave,slot=slot)
                phase='ready';deadline=time.monotonic()+120;gate_time=None;hold_start=None
                while True:
                    now=time.perf_counter_ns(); ready=0;correct=0;live=0;aggregate_pss=0
                    for job in jobs:
                        proc=job['process'];data=job['log'].read_bytes()
                        if job['pid'] is None:
                            job['pid']=published_pid(proc,job['jail']/(firecracker.name+'.pid'))
                        if job['pid'] is not None and running(job['pid']):
                            live+=1
                            candidate=status(job['pid'])
                            if candidate:
                                if pin_cpus and candidate['Cpus_allowed_list']!=str(job['pinned_cpu']):
                                    raise RuntimeError('VMM affinity differs from assigned CPU')
                                aggregate_pss+=candidate['pss_kib']
                                own=str(job['pid']) in (job['cgroup']/'cgroup.procs').read_text().split()
                                if (own and candidate['NoNewPrivs']=='1' and candidate['Seccomp']=='2' and
                                    candidate['CapEff']=='0000000000000000' and
                                    all(int(v)==job['uid'] for v in candidate['Uid'].split()+candidate['Gid'].split()) and
                                    all(candidate['namespaces'][name]!=host_ns[name] for name in host_ns)):
                                    job['status']=candidate
                            if b'SABLE_VM_READY' in data and job['status']:
                                ready+=1
                                if job['ready'] is None:job['ready']=now;event('ready',wave=wave,id=job['id'])
                            if b'SABLE_VM_IDLE' in data:
                                correct+=1
                                if job['correct'] is None:job['correct']=now;event('work_correct_waiting',wave=wave,id=job['id'])
                        elif job['pid'] is not None and phase!='exit':
                            raise RuntimeError('guest exited before release: '+job['id'])
                        if b'SABLE_VM_FAILED' in data or b'SABLE_RESET_FAILED' in data or b'SABLE_PROTOCOL_FAILED' in data:
                            raise RuntimeError('guest correctness/protocol failure: '+job['id'])
                    metrics['min_host_available_bytes']=min(metrics['min_host_available_bytes'],memory_available())
                    metrics['peak_parent_current_bytes']=max(metrics['peak_parent_current_bytes'],int((parent/'memory.current').read_text()))
                    metrics['peak_sampled_aggregate_vmm_pss_kib']=max(metrics['peak_sampled_aggregate_vmm_pss_kib'],aggregate_pss)
                    metrics['max_simultaneously_ready']=max(metrics['max_simultaneously_ready'],ready)
                    metrics['max_simultaneously_work_complete']=max(metrics['max_simultaneously_work_complete'],correct)
                    if phase=='ready' and ready==count:
                        if hold_start is None:
                            hold_start=time.monotonic();event('all_ready_resident',wave=wave,count=count)
                            metrics['ready_parent_current_bytes']=int((parent/'memory.current').read_text())
                            metrics['ready_aggregate_vmm_pss_kib']=aggregate_pss
                            metrics['all_ready_seconds']=(now-requested)/1e9
                            metrics['idle_cpu_before']=(parent/'cpu.stat').read_text()
                        if time.monotonic()-hold_start>=3:
                            metrics['idle_cpu_after']=(parent/'cpu.stat').read_text()
                            metrics['ready_hold_seconds']=time.monotonic()-hold_start
                            gate_time=time.perf_counter_ns()
                            for job in jobs:job['process'].stdin.write(b'RUN\n');job['process'].stdin.flush()
                            phase='work';hold_start=None
                            event('work_released',wave=wave,count=count)
                    elif phase=='work' and correct==count:
                        if hold_start is None:
                            hold_start=time.monotonic();metrics['all_work_correct_seconds']=(now-gate_time)/1e9
                            metrics['work_cpu_after']=(parent/'cpu.stat').read_text()
                            event('all_work_correct_resident',wave=wave,count=count)
                        if time.monotonic()-hold_start>=1:
                            for job in jobs:job['process'].stdin.write(b'EXIT\n');job['process'].stdin.flush()
                            phase='exit';event('exit_released',wave=wave,count=count)
                    elif phase=='exit' and live==0:break
                    if memory_available()<384*1024*1024:raise RuntimeError('host memory reserve crossed; no capacity pass')
                    if time.monotonic()>deadline:raise TimeoutError('wave deadline; phase='+phase)
                    time.sleep(.025)
                records=[]
                for job in jobs:
                    data=job['log'].read_bytes()
                    if b'SABLE_VM_OK' not in data or b'Firecracker exiting successfully. exit_code=0' not in data:
                        raise RuntimeError('missing verified VMM exit: '+job['id'])
                    if data.count(b'SABLE_WORK_OK')!=repetitions or b'SABLE_REPETITIONS_OK' not in data:
                        raise RuntimeError('wrong number of checked CPU workloads: '+job['id'])
                    cg={name:(job['cgroup']/name).read_text().strip() for name in ['memory.peak','memory.events','cpu.stat','cpu.max','pids.events']}
                    if int(dict(line.split() for line in cg['memory.events'].splitlines())['oom_kill']):
                        raise RuntimeError('job cgroup OOM: '+job['id'])
                    records.append(dict(id=job['id'],uid=job['uid'],status=job['status'],cgroup=cg,pinned_cpu=job['pinned_cpu'],
                                        ready_from_wave_seconds=(job['ready']-requested)/1e9,
                                        work_after_gate_seconds=(job['correct']-gate_time)/1e9))
                metrics['host_cpu_before']=before_cpu;metrics['host_cpu_after']=Path('/proc/stat').read_text().splitlines()[0]
                metrics['parent_memory_events']=(parent/'memory.events').read_text()
                metrics['parent_cpu_stat']=(parent/'cpu.stat').read_text()
                metrics['memory_pressure']=Path('/proc/pressure/memory').read_text()
                result=dict(wave=wave,requested_concurrency=count,all_correct=True,guest_mib=guest_mib,work_repetitions=repetitions,metrics=metrics,jobs=records)
            except Exception as error:
                save(output/f'wave-{wave:02}-failure.json',dict(error=str(error),concurrency=count,metrics=metrics))
                (output/f'wave-{wave:02}-host-kernel.log').write_bytes(command(['dmesg','--ctime']))
                event('wave_failed',wave=wave,error=str(error));raise
            finally:
                cleanup_errors=cleanup_density_jobs(jobs)
                if cleanup_errors:
                    save(output/f'wave-{wave:02}-cleanup-failure.json',dict(errors=cleanup_errors))
                    raise RuntimeError('owned wave cleanup incomplete: '+json.dumps(cleanup_errors))
            result['full_wave_seconds']=(time.perf_counter_ns()-requested)/1e9
            save(output/f'wave-{wave:02}.json',result);waves.append(result)
            event('wave_cleaned',wave=wave,count=count,full_wave_seconds=result['full_wave_seconds'])
        save(output/'density-summary.json',dict(scope='trusted CPU jobs, serial-gated resident guests, not iPhone apps',
              all_correct=True,concurrency_levels=levels,guest_mib=guest_mib,
              work_repetitions=repetitions,
              max_verified_concurrency=max(w['metrics']['max_simultaneously_work_complete'] for w in waves),
              app_workflows=0,competitive_speedup_claim=False))
    finally:events.close()


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output',required=True,type=Path)
    parser.add_argument('--blocks',type=int,default=31)
    parser.add_argument('--concurrency',type=int,nargs='+',help='Separate gated density experiment, e.g. 1 4 16 32 64 96 150')
    parser.add_argument('--guest-mib',type=int,choices=[64,128,256],default=128)
    parser.add_argument('--work-repetitions',type=int,default=1,help='Repeat checked CPU workload per gated guest; not an app workload')
    parser.add_argument('--pin-cpus',action='store_true',help='Experimental round-robin single-host-CPU affinity per gated VMM')
    options=parser.parse_args()
    if os.geteuid()!=0 or platform.system()!='Linux' or platform.machine()!='x86_64':
        parser.error('requires root on the owned x86-64 Linux/KVM research instance')
    if options.blocks<1 or options.blocks>101: parser.error('blocks must be 1..101')
    if options.concurrency and any(n<1 or n>150 for n in options.concurrency):parser.error('concurrency must be 1..150')
    if not 1<=options.work_repetitions<=100:parser.error('work repetitions must be 1..100')
    if not options.concurrency and options.work_repetitions!=1:parser.error('repetitions require gated concurrency mode')
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
    if options.concurrency:
        init=(initrd/'init').read_text().replace('echo SABLE_VM_READY','''echo SABLE_VM_READY
IFS= read -r token < /dev/ttyS0
if [ "$token" != RUN ]; then echo SABLE_PROTOCOL_FAILED; reboot -f; fi''')
        init=init.replace('  echo SABLE_VM_OK','''  echo SABLE_VM_IDLE
  IFS= read -r token < /dev/ttyS0
  if [ "$token" != EXIT ]; then echo SABLE_PROTOCOL_FAILED; reboot -f; fi
  echo SABLE_VM_OK''')
        init=init.replace('if /drop /payload/busybox sh /payload/workload.sh /payload /tmp/work; then',
                          'if /drop /payload/busybox sh /payload/repeat.sh; then')
        repeat='''#!/bin/sh
set -eu
i=0
while [ "$i" -lt REPETITIONS ]; do
  /payload/busybox sh /payload/workload.sh /payload /tmp/work
  i=$((i+1))
done
echo SABLE_REPETITIONS_OK
'''.replace('REPETITIONS',str(options.work_repetitions),1)
        (initrd/'payload/repeat.sh').write_text(repeat)
        (initrd/'init').write_text(init)
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
    info['density_levels']=options.concurrency
    info['density_guest_mib']=options.guest_mib if options.concurrency else None
    info['work_repetitions']=options.work_repetitions
    info['pin_policy']='round_robin_single_cpu_per_vmm' if options.pin_cpus else 'host_scheduler'
    info['host_cpu_count']=os.cpu_count()
    info['host_meminfo']={line.split(':')[0]:line.split(':')[1].strip() for line in Path('/proc/meminfo').read_text().splitlines()
                          if line.split(':')[0] in ['MemTotal','MemAvailable','SwapTotal','SwapFree']}
    save(output/'inputs.json',info)
    host_ns={name:os.readlink('/proc/self/ns/'+name) for name in ['pid','mnt','net']}
    info['host_namespaces']=host_ns;save(output/'inputs.json',info)
    # These changes are scoped to this disposable instance, not a shared host.
    cgroot=Path('/sys/fs/cgroup')
    with (cgroot/'cgroup.subtree_control').open('w') as f: f.write('+cpu +memory +pids')
    parent=cgroot/run_id
    records=[];rng=random.Random(0x5ab1e)
    netns=run_id
    netns_created=False
    journal=output/'samples.jsonl'
    try:
        parent.mkdir()
        (parent/'cgroup.subtree_control').write_text('+cpu +memory +pids')
        command(['ip','netns','add',netns]);netns_created=True
        if options.concurrency:
            density(base,output,firecracker,jailer,parent,host_ns,options.concurrency,options.guest_mib,options.work_repetitions,options.pin_cpus)
            return
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
                            if mode!='process':
                                if vmm_pid is None:
                                    vmm_pid=published_pid(process,pidfile)
                                    if vmm_pid is not None:
                                        recovery['owned_vmm_pid']=vmm_pid;save(base/'recovery.json',recovery)
                                candidate=status(vmm_pid) if vmm_pid is not None else None
                                own=(vmm_pid is not None and cgroup.exists() and str(vmm_pid) in (cgroup/'cgroup.procs').read_text().split())
                                if own and candidate and candidate['NoNewPrivs']=='1' and candidate['Seccomp']=='2' and int(candidate['Uid'].split()[0])==uid:
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
        if netns_created:command(['ip','netns','delete',netns])
        if parent.exists():parent.rmdir()
        cleanup={'remaining_owned_cgroups':len(list(cgroot.glob(run_id+'/*'))),'owned_netns_remaining':Path('/run/netns',netns).exists(),
                 'remaining_owned_network_namespaces':len(list(Path('/run/netns').glob(run_id+'*'))),
                 'remaining_owned_jails':len(list((base/'jails').glob('*/*'))) if (base/'jails').exists() else 0}
        save(output/'cleanup.json',cleanup)
    print((output/'summary.json').read_text())


if __name__=='__main__':main()
