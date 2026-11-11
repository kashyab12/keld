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


D_EDGES=[0,1<<63,0x7ff0000000000000,0xfff0000000000000,0x7ff8000000000000,0x7ff8000000000123,0xfff8000000000456,
         0x7ff0000000000001,0xfff4000000000789,1,0x8000000000000001,0x000fffffffffffff,0x0010000000000000,0x8010000000000000,
         0x7fefffffffffffff,0xffefffffffffffff,0x3ff0000000000000,0xbff8000000000000,0x3ff0000000000001,0x3cb0000000000000,
         0x3ca0000000000000,0x4340000000000000,0x4330000000000001,0x3fe0000000000000,0xbfe0000000000000,0x4004000000000000,
         0x41e0000000000000,0xc1e0000000000000,0x43e0000000000000,0xc3e0000000000000,0x41efffffffe00000,0x3fd5555555555555,0x0008000000000000]
S_EDGES=[0,0x80000000,0x7f800000,0xff800000,0x7fc00000,0x7fc00123,0xffc00456,0x7f800001,0xffa00789,1,0x80000001,0x007fffff,
         0x00800000,0x80800000,0x7f7fffff,0xff7fffff,0x3f800000,0xbfc00000,0x3f800001,0x33800000,0x33000000,0x4b800000,0x3f000000,
         0xbf000000,0x4f000000,0xcf000000,0x5f000000,0xdf000000,0x3eaaaaab,0x00400000]
FP_SVC=0xd4001001


def corpus(seed, count):
    rng=random.Random(seed)
    cases=[]
    edges=[0,1,0xffffffff,0x80000000,0x7fffffff,0xffffffffffffffff,0x8000000000000000,0x7fffffffffffffff]
    def vreg():
        pick=rng.randrange(4)
        low=rng.choice(D_EDGES) if pick==0 else (rng.getrandbits(32)<<32|rng.choice(S_EDGES)) if pick==1 else rng.getrandbits(64)
        return rng.getrandbits(64)<<64|low
    def case(words, kind):
        regs=[rng.choice(edges) if rng.randrange(2) else rng.getrandbits(64) for _ in range(31)]
        cases.append(dict(kind=kind,words=words+[SVC],x=regs,sp=STACK+2048,nzcv=rng.randrange(16)<<28,data=rng.randbytes(512),stack=rng.randbytes(4096),
                          v=[vreg() for _ in range(32)],fpcr=0,fpsr=rng.choice([0,0x9f,0x08000000,rng.randrange(32)])))
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
    # CSEL/CSINC/CSINV/CSNEG, both widths, all conditions/flags, ZR and aliases.
    for wide in [0,1]:
        for invert in [0,1]:
            for increment in [0,1]:
                for flags in range(16):
                    for condition in range(16):
                        dst=rng.choice([0,1,31]);src=rng.choice([0,1,31]);rhs=rng.choice([0,1,31])
                        case([0x1a800000|wide<<31|invert<<30|rhs<<16|condition<<12|increment<<10|src<<5|dst],"conditional_select_zr_aliases")
                        cases[-1]["nzcv"]=flags<<28
    # TBZ/TBNZ: every bit, zero-register, both outcomes and signed displacements.
    for bit in range(64):
        for nonzero in [0,1]:
            for reg in [0,31]:
                for value in [0,1]:
                    for backward in [0,1]:
                        op=0x36000000|(bit>>5)<<31|nonzero<<24|(bit&31)<<19|reg
                        words=([0x14000002,SVC,op|0x3fff<<5,0x91000508] if backward else [op|2<<5,0x91000508])
                        case(words,"test_bit_branches_signed_zr")
                        if reg!=31:
                            cases[-1]["x"][reg]=(cases[-1]["x"][reg]&~(1<<bit))|(value<<bit)
    # Every valid UBFM immediate pair in both widths, with normal/ZR/alias roles.
    # Includes the UBFIZ emitted by Ubuntu Clang 18 for the protocol Bool caller.
    for wide in [0,1]:
        for r in range(64 if wide else 32):
            for s in range(64 if wide else 32):
                dst,src=[(0,1),(0,0),(31,1),(0,31)][(r+s)%4]
                case([0x53000000|wide<<31|wide<<22|r<<16|s<<10|src<<5|dst],"unsigned_bitfield_all_immediates")
    for value in edges:
        case([0x531e7500],"ubuntu_clang_protocol_ubfiz")
        cases[-1]["x"][8]=value
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
    # Compiler-emitted LDRSW literal: both signs, XZR and backwards code data.
    for value in [0,1,0x7fffffff,0x80000000,0xffffffff]:
        for reg in [0,8,31]:
            case([0x98000000|(((DATA-BASE)//4)<<5)|reg],"signed_word_literal_forward")
            cases[-1]['data']=struct.pack('<I',value)+cases[-1]['data'][4:]
            case([0x14000002,value,0x98000000|(0x7ffff<<5)|reg],"signed_word_literal_backward")
    # BL/RET, including a nonzero return address, without overwriting random X30.
    case([0x94000003,0x14000004,0xd503201f,0x91000400,0xd65f03c0],"call_return")
    for size,baseword in [(4,0x18000000),(8,0x58000000)]:
        case([baseword|(((DATA-BASE)//4)<<5)|8],"literal_data_load")
        case([0xd503201f,baseword|(((-1)&0x7ffff)<<5)|8],"literal_code_load_backward")
    case([0xd63f03c0,0x14000004,0xd503201f,0x91000400,0xd65f03c0],"indirect_call_x30_alias")
    cases[-1]["x"][30]=BASE+12
    case([0xd61f0120,0xd503201f,0x91000400],"indirect_branch")
    cases[-1]["x"][9]=BASE+8
    fp_corpus(rng,case,cases)
    int_corpus(rng,case,cases)
    return cases


def int_corpus(rng,case,cases):
    """Integer forms clang emits beyond the original subset (division, variable
    shifts, multiplies, SBFM, EXTR, 1-source bit ops, sign-extending loads)."""
    r=lambda:rng.randrange(32)
    for wide in [0,1]:
        for kind in [2,3,8,9,10,11]:
            for _ in range(200):case([0x1ac00000|wide<<31|r()<<16|kind<<10|r()<<5|r()],'int_dp2_%d'%kind)
            # Signed division corner: INT_MIN / -1 and division by zero.
            for a,b in [(1<<63 if wide else 1<<31,(1<<64)-1),(5,0),(0,0)]:
                case([0x1ac00000|wide<<31|2<<16|kind<<10|1<<5|0],'int_dp2_edges');cases[-1]['x'][1]=a;cases[-1]['x'][2]=b
        for kind in [0,1,2,3,4,5]:
            if kind==3 and not wide:continue
            for _ in range(150):case([0x5ac00000|wide<<31|kind<<10|r()<<5|r()],'int_dp1_%d'%kind)
        for op31,o0s in [(0,[0,1])]+([(1,[0,1]),(5,[0,1]),(2,[0]),(6,[0])] if wide else []):
            for o0 in o0s:
                for _ in range(200):
                    ra=31 if op31 in (2,6) else r()
                    case([0x1b000000|wide<<31|op31<<21|r()<<16|o0<<15|ra<<10|r()<<5|r()],'int_madd_family')
        for imm_r in range(64 if wide else 32):
            for imm_s in range(64 if wide else 32):
                dst,src=[(0,1),(0,0),(31,1),(0,31)][(imm_r+imm_s)%4]
                case([0x13000000|wide<<31|wide<<22|imm_r<<16|imm_s<<10|src<<5|dst],'int_sbfm_all_immediates')
        for lsb in range(64 if wide else 32):
            case([0x13800000|wide<<31|wide<<22|r()<<16|lsb<<10|r()<<5|r()],'int_extr')
    for size,opc in [(0,2),(0,3),(1,2),(1,3),(2,2)]:
        for base in [20,31]:
            case([0x39000000|size<<30|opc<<22|5<<10|base<<5|r()],'int_signed_load_unsigned');cases[-1]['x'][20]=DATA+128
            for mode in [0,1,3]:
                for off in [-16,-3,0,5,16]:
                    if base==31 and off%16:continue
                    t=rng.choice([0,7,31]) if mode else r()
                    case([0x38000000|size<<30|opc<<22|(off&511)<<12|mode<<10|base<<5|t],'int_signed_load_unscaled_pre_post');cases[-1]['x'][20]=DATA+128
        for option in [2,3,6,7]:
            for scaled in [0,1]:
                case([0x38200800|size<<30|opc<<22|21<<16|option<<13|scaled<<12|20<<5|r()],'int_signed_load_register')
                cases[-1]['x'][20]=DATA+128;cases[-1]['x'][21]=rng.choice([0,1,3,0xffffffffffffffff]) if option in [6,7] else 3


def fp_corpus(rng,case,cases):
    """Every implemented FP/SIMD form with V/FPSR state and IEEE edge inputs."""
    r=lambda:rng.randrange(32)
    def setv(n,value,double):
        pool=D_EDGES if double else S_EDGES
        cases[-1]['v'][n]=(cases[-1]['v'][n]>>64<<64)|(value if value is not None else rng.choice(pool) if rng.randrange(4) else rng.getrandbits(64 if double else 32))
    def edges(double,*regs):
        for n in regs:setv(n,None,double)
    for ftype in [0,1]:
        dbl=ftype==1
        for opcode,name in [(0,'fmul'),(1,'fdiv'),(2,'fadd'),(3,'fsub'),(6,'fmaxnm'),(7,'fminnm'),(8,'fnmul')]:
            for _ in range(300):
                d,n,m=r(),r(),r()
                case([0x1e200800|ftype<<22|m<<16|opcode<<12|n<<5|d],'fp_'+name+('_d' if dbl else '_s'));edges(dbl,n,m)
        for o1 in [0,1]:
            for o0 in [0,1]:
                for _ in range(300):
                    d,n,m,a=r(),r(),r(),r()
                    case([0x1f000000|ftype<<22|o1<<21|m<<16|o0<<15|a<<10|n<<5|d],'fp_fmadd_family'+('_d' if dbl else '_s'));edges(dbl,n,m,a)
        for opcode in [0,1,2,3,9,10,11]:
            for _ in range(150):
                d,n=r(),r()
                case([0x1e204000|ftype<<22|opcode<<15|n<<5|d],'fp_1src_%d'%opcode+('_d' if dbl else '_s'));edges(dbl,n)
        for _ in range(150):
            d,n=r(),r()
            case([0x1e204000|ftype<<22|(4 if dbl else 5)<<15|n<<5|d],'fp_fcvt'+('_d2s' if dbl else '_s2d'));edges(dbl,n)
        for opcode2 in [0,8,16,24]:
            for _ in range(150):
                n,m=r(),(0 if opcode2&8 else r())
                case([0x1e202000|ftype<<22|m<<16|n<<5|opcode2],'fp_fcmp'+('_d' if dbl else '_s'));edges(dbl,n,m)
        for cond in range(16):
            for _ in range(8):
                d,n,m=r(),r(),r()
                case([0x1e200c00|ftype<<22|m<<16|cond<<12|n<<5|d],'fp_fcsel')
        for imm in range(256):
            case([0x1e201000|ftype<<22|imm<<13|r()],'fp_fmov_imm')
        for sf in [0,1]:
            for rmode,opcode in [(0,2),(0,3),(3,0),(3,1),(0,0),(0,1),(0,4),(0,5),(1,0),(1,1),(2,0),(2,1)]:
                for _ in range(200):
                    d,n=r(),r()
                    case([0x1e200000|sf<<31|ftype<<22|rmode<<19|opcode<<16|n<<5|d],'fp_int_conv')
                    if opcode not in (2,3):edges(dbl,n)
            if sf==ftype:
                for opcode in [6,7]:
                    for _ in range(40):case([0x1e200000|sf<<31|ftype<<22|opcode<<16|r()<<5|r()],'fp_fmov_general')
        for u in [0,1]:
            for _ in range(150):
                d,n=r(),r()
                case([0x5e21d800|u<<29|ftype<<22|n<<5|d],'fp_scvtf_vector')
    for q in [0,1]:
        for _ in range(60):
            n=r();case([0x0ea01c00|q<<30|n<<16|n<<5|r()],'simd_mov_vector')
        for op in [0,1]:
            for imm in range(0,256,7):
                case([0x0f00e400|q<<30|op<<29|(imm>>5)<<16|(imm&31)<<5|r()],'simd_movi')
    for word in [0x0f10a400,0x0f20a400]:
        for _ in range(60):
            case([word|r()<<5|r()],'simd_sxtl')
    for reg,op2 in [(0,0),(1,1)]:
        for _ in range(20):
            t=r()
            case([0xd53b4400|op2<<5|t],'fp_mrs')
        for _ in range(20):
            t=rng.choice([0,1,2,31])
            case([0xd51b4400|op2<<5|t],'fp_msr')
            if t!=31: cases[-1]['x'][t]=0 if op2==0 else rng.getrandbits(64)&0x0800009f  # AArch64 FPSR bits 31:28 are RES0
    # Loads/stores: B/H/S/D/Q, unsigned, unscaled, pre/post, register, pairs, literal.
    for size,opc_load,opc_store in [(0,1,0),(1,1,0),(2,1,0),(3,1,0),(0,3,2)]:
        bytes_=16 if opc_load==3 else 1<<size
        for opc in [opc_load,opc_store]:
            for base in [20,31]:
                case([0x3d000000|size<<30|opc<<22|3<<10|base<<5|r()],'fp_mem_unsigned');cases[-1]['x'][20]=DATA+128
                for mode in [0,1,3]:
                    for off in [-16,-1,0,7,16]:
                        if base==31 and off%16: continue
                        case([0x3c000000|size<<30|opc<<22|(off&511)<<12|mode<<10|base<<5|r()],'fp_mem_unscaled_pre_post');cases[-1]['x'][20]=DATA+128
            for option in [2,3,6,7]:
                for scaled in [0,1]:
                    case([0x3c200800|size<<30|opc<<22|21<<16|option<<13|scaled<<12|20<<5|r()],'fp_mem_register')
                    cases[-1]['x'][20]=DATA+128;cases[-1]['x'][21]=rng.choice([0,1,3,0xffffffffffffffff]) if option in [6,7] else 3
    for opc in [0,1,2]:
        for mode in [1,2,3]:
            for load in [0,1]:
                for base in [20,31]:
                    for imm in [-2,0,3]:
                        t=r();t2=(t+1+rng.randrange(31))%32
                        case([0x2c000000|opc<<30|mode<<23|load<<22|(imm&127)<<15|t2<<10|base<<5|t],'fp_mem_pair');cases[-1]['x'][20]=DATA+128
        case([0x1c000000|opc<<30|(((DATA-BASE)//4)<<5)|r()],'fp_literal')
    # Two-instruction ABI sequences: compare then select, convert round trip.
    for _ in range(100):
        n,m,d=r(),r(),r()
        case([0x1e602000|m<<16|n<<5,0x1e600c00|m<<16|rng.randrange(16)<<12|n<<5|d],'fp_fcmp_fcsel_sequence');edges(True,n,m)


def vbytes(c):
    return b''.join(v.to_bytes(16,'little') for v in c['v'])


def oracle(case):
    mu=Uc(UC_ARCH_ARM64,UC_MODE_ARM)
    mu.mem_map(BASE,0x10000);mu.mem_map(DATA,0x1000);mu.mem_map(STACK,1024*1024)
    mu.mem_write(BASE,struct.pack('<'+'I'*len(case['words']),*case['words']))
    mu.mem_write(DATA,case['data']);mu.mem_write(STACK,case['stack'])
    for i,value in enumerate(case['x']):mu.reg_write(getattr(arm,'UC_ARM64_REG_X'+str(i)),value)
    mu.reg_write(arm.UC_ARM64_REG_SP,case['sp']);mu.reg_write(arm.UC_ARM64_REG_NZCV,case['nzcv'])
    for i,value in enumerate(case['v']):mu.reg_write(arm.UC_ARM64_REG_Q0+i,value)
    mu.reg_write(arm.UC_ARM64_REG_FPCR,case['fpcr']);mu.reg_write(arm.UC_ARM64_REG_FPSR,case['fpsr'])
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
    return dict(x=[mu.reg_read(getattr(arm,'UC_ARM64_REG_X'+str(i))) for i in range(31)],sp=mu.reg_read(arm.UC_ARM64_REG_SP),nzcv=mu.reg_read(arm.UC_ARM64_REG_NZCV),v=b''.join(mu.reg_read(arm.UC_ARM64_REG_Q0+i).to_bytes(16,'little') for i in range(32)).hex(),fpcr=mu.reg_read(arm.UC_ARM64_REG_FPCR),fpsr=mu.reg_read(arm.UC_ARM64_REG_FPSR),pc=mu.reg_read(arm.UC_ARM64_REG_PC),instructions=instructions,data=bytes(mu.mem_read(DATA,512)).hex(),stack=bytes(mu.mem_read(STACK,4096)).hex())


def main():
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('--binary',default='target/release/examples/oracle_batch');p.add_argument('--engine',choices=['interp','jit'],default='interp');p.add_argument('--cases',type=int,default=2000);p.add_argument('--seed',type=int,default=12648430);p.add_argument('--output',type=Path,default=Path('artifacts/oracle-summary.json'));o=p.parse_args()
    input_started=time.monotonic()
    cases=corpus(o.seed,o.cases)
    payload=bytearray(b'SABLE002'+struct.pack('<I',len(cases)))
    for c in cases:
        payload.extend(struct.pack('<I33Q',len(c['words']),*c['x'],c['sp'],c['nzcv'])+vbytes(c)+struct.pack('<2Q',c['fpcr'],c['fpsr'])+struct.pack('<'+'I'*len(c['words']),*c['words'])+c['data']+c['stack'])
    input_elapsed=time.monotonic()-input_started
    start=time.monotonic();result=subprocess.run([o.binary,o.engine],input=payload,capture_output=True,timeout=900)
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
    summary=dict(oracle='Unicorn',version=unicorn.__version__,engine=o.engine,seed=o.seed,cases=len(cases),kinds=kinds,input_sha256=hashlib.sha256(payload).hexdigest(),elapsed_seconds=time.monotonic()-start,input_build_elapsed_seconds=input_elapsed,full_registers_flags_memory_equal=True)
    o.output.write_text(json.dumps(summary,indent=2)+'\n');print(json.dumps(summary))


if __name__=='__main__':main()
