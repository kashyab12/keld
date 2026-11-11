#!/usr/bin/env python3
"""Unprivileged regression for the jailer's create-before-write PID publication."""
import importlib.util
import ast
from pathlib import Path
import random
import shutil
import subprocess
import sys
import tempfile
from types import SimpleNamespace

spec=importlib.util.spec_from_file_location("firecracker_bench",Path(__file__).with_name("firecracker-bench.py"))
bench=importlib.util.module_from_spec(spec);spec.loader.exec_module(bench)
with tempfile.TemporaryDirectory(prefix="sable-pid-check-") as folder:
    path=Path(folder)/"pid"
    writer=subprocess.Popen([sys.executable,"-c", "import os,sys;from pathlib import Path;p=Path(sys.argv[1]);p.touch();print('created',flush=True);sys.stdin.readline();p.write_text(str(os.getpid()))",str(path)],stdin=subprocess.PIPE,stdout=subprocess.PIPE,text=True)
    try:
        assert writer.stdout.readline().strip()=="created"
        assert path.read_text()==""
        assert bench.published_pid(writer,path) is None, "must not parse an incomplete publication"
        writer.stdin.write("publish\n");writer.stdin.flush();writer.wait(timeout=5)
        assert bench.published_pid(writer,path)==writer.pid
        for value in ["", "0", "1", "-2", "12x", "123\n456", "１２３"]:
            path.write_text(value)
            try:bench.published_pid(writer,path)
            except RuntimeError:pass
            else:raise AssertionError("accepted invalid PID: "+repr(value))
        path.unlink()
        try:bench.published_pid(writer,path)
        except FileNotFoundError:pass
        else:raise AssertionError("missing publication accepted")
        writer.returncode=4
        try:bench.published_pid(writer,path)
        except RuntimeError:pass
        else:raise AssertionError("failed launcher accepted")
    finally:
        if writer.poll() is None:writer.kill();writer.wait(timeout=5)
        writer.stdin.close();writer.stdout.close()
print("PID publication: delayed write, malformed/missing PID and failed launcher checks passed")

# Fault-inject the actual pool acquisition/rollback control flow. All paths and
# namespace commands below are replaced by private files/fakes, never real KVM,
# cgroups or namespaces. Ordinary files stand in for virtual cgroup attributes.
tree=ast.parse(Path(__file__).with_name('firecracker-bench.py').read_text())
main=next(n for n in tree.body if isinstance(n,ast.FunctionDef) and n.name=='main')
start=next(i for i,n in enumerate(main.body) if isinstance(n,ast.Assign) and any(isinstance(t,ast.Name) and t.id=='cgroot' for t in n.targets))
end=next(i for i,n in enumerate(main.body[start:],start) if isinstance(n,ast.Try))
wrapper=ast.FunctionDef(name='rollback',args=ast.arguments(posonlyargs=[],args=[],kwonlyargs=[],kw_defaults=[],defaults=[]),
                       body=main.body[start:end+1],decorator_list=[])
code=compile(ast.fix_missing_locations(ast.Module(body=[wrapper],type_ignores=[])),'pool-rollback-fault-injection','exec')
with tempfile.TemporaryDirectory(prefix='sable-pool-check-') as folder:
    owned=Path(folder);cg=owned/'cgroups';cg.mkdir();(cg/'cgroup.subtree_control').touch()
    output=owned/'output';output.mkdir();calls=[]
    class CgroupFiles(type(Path())):
        def rmdir(self):
            assert self.is_relative_to(owned)
            attribute=self/'cgroup.subtree_control'
            if attribute.exists():attribute.unlink()
            super().rmdir()
    def private_path(value,*parts):
        assert value in ['/sys/fs/cgroup','/run/netns'], value
        return CgroupFiles(cg) if value=='/sys/fs/cgroup' else owned.joinpath('netns',*parts)
    def namespace_failure(args,**kwargs):
        calls.append(args)
        assert args==['ip','netns','add','owned-test-run']
        raise RuntimeError('injected namespace-add failure')
    env=dict(Path=private_path,run_id='owned-test-run',output=output,base=owned,random=random,
             options=SimpleNamespace(concurrency=None,blocks=0),command=namespace_failure,save=bench.save,shutil=shutil)
    exec(code,env)
    try:env['rollback']()
    except RuntimeError as error:assert str(error)=='injected namespace-add failure'
    else:raise AssertionError('namespace failure was suppressed')
    assert not (cg/'owned-test-run').exists(), 'owned parent leaked after namespace setup failure'
    assert (output/'cleanup.json').exists(), 'rollback receipt missing'
    assert len(calls)==1, calls
print('Pool rollback: namespace-add failure removed the simulated owned parent and wrote cleanup; no privileged operations')

# A failed buffered release is flushed again by close(). It must not skip the
# later live child or any independent jail/namespace cleanup operation.
for inject_failure in [False,True]:
    with tempfile.TemporaryDirectory(prefix='sable-pipe-check-') as folder:
        owned=Path(folder);children=[];jobs=[];deletions=[];original_command=bench.command
        def absent_cgroup():return False
        def inaccessible_cgroup():raise PermissionError('injected cgroup failure')
        def fake_namespace_delete(args,**kwargs):
            assert args[:3]==['ip','netns','delete']
            deletions.append(args[-1])
            if inject_failure and args[-1]=='owned-0':raise OSError('injected namespace cleanup failure')
        try:
            dead=subprocess.Popen([sys.executable,'-c','pass'],stdin=subprocess.PIPE,start_new_session=True)
            children.append(dead);dead.wait(timeout=5)
            live=subprocess.Popen([sys.executable,'-c','import sys;sys.stdin.read()'],stdin=subprocess.PIPE,start_new_session=True)
            children.append(live)
            for i,proc in enumerate(children):
                jail=owned/f'job-{i}'/'root';jail.mkdir(parents=True)
                jobs.append(dict(id=f'owned-{i}',process=proc,jail=jail,namespace=f'owned-{i}',namespace_created=True,
                                 cgroup=SimpleNamespace(exists=inaccessible_cgroup if inject_failure and i==0 else absent_cgroup)))
            bench.command=fake_namespace_delete
            try:
                try:dead.stdin.write(b'RUN\n');dead.stdin.flush()
                finally:errors=bench.cleanup_density_jobs(jobs)
            except BrokenPipeError:pass  # The original release failure survives cleanup.
            else:raise AssertionError('original failed release was suppressed')
            assert all(proc.poll() is not None and proc.stdin.closed for proc in children)
            assert all(not job['jail'].parent.exists() for job in jobs)
            assert deletions==['owned-0','owned-1'], deletions
            assert [error['resource'] for error in errors]==(['cgroup','network_namespace'] if inject_failure else []),errors
        finally:
            bench.command=original_command
            for proc in children:
                if proc.poll() is None:proc.kill();proc.wait(timeout=5)
                if proc.stdin:
                    try:proc.stdin.close()
                    except BrokenPipeError:pass
print('Broken release: all owned children reaped, jails removed, namespace cleanup visited; independent errors aggregated')
