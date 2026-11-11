#!/usr/bin/env python3
"""Independent ARM64 differential test. pip install unicorn==2.1.4 in a venv.

Build cargo build --release --example oracle_batch first. Unicorn is a test oracle,
never part of Sable's runtime. Persist failing input before raising an exception.
"""
import argparse
import hashlib
import json
from pathlib import Path
import random
import struct
import subprocess
import time

from unicorn import Uc, UC_ARCH_ARM64, UC_MODE_ARM, UC_HOOK_INTR, UC_HOOK_CODE
from unicorn import arm64_const as arm
import unicorn

BASE=0x1000
DATA=0x20000
STACK=0x70000000
SVC=0xd4001001


def corpus(seed, count):
    rng=random.Random(seed)
    cases=[]
    edges=[0,1,0xffffffff,0x80000000,0x7fffffff,0xffffffffffffffff,0x8000000000000000,0x7fffffffffffffff]
    def case(words, kind):
        regs=[rng.choice(edges) if rng.randrange(2) else rng.getrandbits(64) for _ in range(31)]
        cases.append(dict(kind=kind,words=words+[SVC],x=regs,sp=STACK+2048,nzcv=rng.randrange(16)<<28,data=rng.randbytes(512),stack=rng.randbytes(4096)))
    for _ in range(count):
        words=[]
        for _ in range(32):
            wide=rng.randrange(2);dst=rng.randrange(32);src=rng.randrange(32);rhs=rng.randrange(32);amount=rng.randrange(64 if wide else 32)
            kind=rng.randrange(5)
            if kind==0:
                words.append(0x11000000|wide<<31|rng.randrange(2)<<30|rng.randrange(2)<<29|rng.randrange(2)<<22|rng.randrange(4096)<<10|src<<5|dst)
            elif kind==1:
                words.append(0x0b000000|wide<<31|rng.randrange(2)<<30|rng.randrange(2)<<29|rng.randrange(3)<<22|rhs<<16|amount<<10|src<<5|dst)
            elif kind==2:
                words.append(0x0a000000|wide<<31|rng.randrange(4)<<29|rng.randrange(4)<<22|rng.randrange(2)<<21|rhs<<16|amount<<10|src<<5|dst)
            elif kind==3:
                words.append(0x12800000|wide<<31|rng.choice([0,2,3])<<29|rng.randrange(4 if wide else 2)<<21|rng.randrange(65536)<<5|dst)
            else:
                # Conditional branch skips exactly one ADD; covers all 16 conditions.
                words.extend([0x54000040|rng.randrange(16),0x91000400|src<<5|dst])
        case(words,"arithmetic_flags_conditions")
    # All conditions and all incoming NZCV combinations, deterministic outcomes.
    for flags in range(16):
        for condition in range(16):
            case([0x54000040|condition,0x91000400],"condition_truth_table")
            cases[-1]["nzcv"]=flags<<28
    # Logical immediates built as rotated runs within repeated elements. The
    # independent CPU validates masks, width, flags and SP/ZR operand roles.
    for wide in [0,1]:
        for power in range(1,7 if wide else 6):
            width=1<<power
            for ones in sorted({1,width//2,width-1}):
                for rotation in sorted({0,1,width-1}):
                    for kind in range(4):
                        for dst in [8,31]:
                            for src in [9,31]:
                                n=int(width==64);imms=((~(width*2-1))&63)|(ones-1)
                                case([0x12000000|wide<<31|kind<<29|n<<22|rotation<<16|imms<<10|src<<5|dst],"logical_immediate_masks_sp_zr")
    # Both CCMP/CCMN operand forms, all conditions and all incoming flags.
    for wide in [0,1]:
        for subtract in [0,1]:
            for immediate in [0,1]:
                for flags in range(16):
                    for condition in range(16):
                        rhs=rng.choice([0,1,15,31]);src=rng.choice([0,1,31])
                        case([0x3a400000|wide<<31|subtract<<30|rhs<<16|condition<<12|immediate<<11|src<<5|rng.randrange(16)],"conditional_compare_flags")
                        cases[-1]["nzcv"]=flags<<28
    # Memory widths, offsets, unaligned data access, signed offsets and writeback.
    for size in range(4):
        for mode in range(4):
            for load in range(2):
                for offset in [-15,-1,0,1,15]:
                    op=0x38000000|size<<30|load<<22|(offset&511)<<12|mode<<10|20<<5|3
                    if mode==2: continue  # LDTR/STTR deliberately unsupported.
                    case([op],"memory_unscaled_pre_post")
                    cases[-1]["x"][20]=DATA+128
        for load in range(2):
            case([0x39000000|size<<30|load<<22|3<<10|20<<5|4],"memory_unsigned")
            cases[-1]["x"][20]=DATA+128
            for option in [2,3,6,7]:
                for scaled in [0,1]:
                    case([0x38200800|size<<30|load<<22|21<<16|option<<13|scaled<<12|20<<5|4],"memory_register_offset")
                    cases[-1]["x"][20]=DATA+128;cases[-1]["x"][21]=rng.choice([0,1,3,0xffffffffffffffff]) if option in [6,7] else 3
    for size in [4,8]:
        for mode in [1,2,3]:
            for load in range(2):
                for base in [20,31]:
                    case([0x28000000|(2 if size==8 else 0)<<30|mode<<23|load<<22|((-2)&127)<<15|5<<10|base<<5|4],"memory_pair")
                    cases[-1]["x"][20]=DATA+128
    # BL/RET, including a nonzero return address, without overwriting random X30.
    case([0x94000003,0x14000004,0xd503201f,0x91000400,0xd65f03c0],"call_return")
    for size,baseword in [(4,0x18000000),(8,0x58000000)]:
        case([baseword|(((DATA-BASE)//4)<<5)|8],"literal_data_load")
        case([0xd503201f,baseword|(((-1)&0x7ffff)<<5)|8],"literal_code_load_backward")
    case([0xd63f03c0,0x14000004,0xd503201f,0x91000400,0xd65f03c0],"indirect_call_x30_alias")
    cases[-1]["x"][30]=BASE+12
    case([0xd61f0120,0xd503201f,0x91000400],"indirect_branch")
    cases[-1]["x"][9]=BASE+8
    return cases


def oracle(case):
    mu=Uc(UC_ARCH_ARM64,UC_MODE_ARM)
    mu.mem_map(BASE,0x10000);mu.mem_map(DATA,0x1000);mu.mem_map(STACK,1024*1024)
    mu.mem_write(BASE,struct.pack('<'+'I'*len(case['words']),*case['words']))
    mu.mem_write(DATA,case['data']);mu.mem_write(STACK,case['stack'])
    for i,value in enumerate(case['x']):mu.reg_write(getattr(arm,'UC_ARM64_REG_X'+str(i)),value)
    mu.reg_write(arm.UC_ARM64_REG_SP,case['sp']);mu.reg_write(arm.UC_ARM64_REG_NZCV,case['nzcv'])
    instructions=0;stopped=False
    def instruction(uc,address,size,_):
        nonlocal instructions
        instructions+=1
    def trap(uc,number,_):
        nonlocal stopped
        if number!=2:raise RuntimeError('Unexpected Unicorn trap '+str(number))
        stopped=True;uc.emu_stop()
    mu.hook_add(UC_HOOK_CODE,instruction);mu.hook_add(UC_HOOK_INTR,trap)
    mu.emu_start(BASE,BASE+len(case['words'])*4,count=10000)
    if not stopped:raise RuntimeError('Oracle did not reach SVC')
    return dict(x=[mu.reg_read(getattr(arm,'UC_ARM64_REG_X'+str(i))) for i in range(31)],sp=mu.reg_read(arm.UC_ARM64_REG_SP),nzcv=mu.reg_read(arm.UC_ARM64_REG_NZCV),pc=mu.reg_read(arm.UC_ARM64_REG_PC),instructions=instructions,data=bytes(mu.mem_read(DATA,512)).hex(),stack=bytes(mu.mem_read(STACK,4096)).hex())


def main():
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('--binary',default='target/release/examples/oracle_batch');p.add_argument('--engine',choices=['interp','jit'],default='interp');p.add_argument('--cases',type=int,default=2000);p.add_argument('--seed',type=int,default=12648430);p.add_argument('--output',type=Path,default=Path('artifacts/oracle-summary.json'));o=p.parse_args()
    cases=corpus(o.seed,o.cases)
    payload=b'SABLE001'+struct.pack('<I',len(cases))
    for c in cases:
        payload+=struct.pack('<I33Q',len(c['words']),*c['x'],c['sp'],c['nzcv'])+struct.pack('<'+'I'*len(c['words']),*c['words'])+c['data']+c['stack']
    start=time.monotonic();result=subprocess.run([o.binary,o.engine],input=payload,capture_output=True,timeout=180)
    o.output.parent.mkdir(parents=True,exist_ok=True)
    if result.returncode:
        o.output.with_suffix('.failed-input.bin').write_bytes(payload);raise RuntimeError(result.stderr.decode())
    actual=[json.loads(line) for line in result.stdout.splitlines()]
    assert len(actual)==len(cases)
    kinds={}
    for i,(c,received) in enumerate(zip(cases,actual)):
        expected=oracle(c);received.pop('id')
        if received!=expected:
            failure=dict(case=i,kind=c['kind'],words=[hex(w) for w in c['words']],initial_x=c['x'],initial_sp=c['sp'],initial_nzcv=c['nzcv'],actual=received,expected=expected)
            o.output.with_suffix('.failure.json').write_text(json.dumps(failure,indent=2)+'\n')
            raise AssertionError(f"case {i}, {c['kind']}: differing fields {[k for k in expected if expected[k]!=received[k]]}")
        kinds[c['kind']]=kinds.get(c['kind'],0)+1
    summary=dict(oracle='Unicorn',version=unicorn.__version__,engine=o.engine,seed=o.seed,cases=len(cases),kinds=kinds,input_sha256=hashlib.sha256(payload).hexdigest(),elapsed_seconds=time.monotonic()-start,full_registers_flags_memory_equal=True)
    o.output.write_text(json.dumps(summary,indent=2)+'\n');print(json.dumps(summary))


if __name__=='__main__':main()
