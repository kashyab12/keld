#!/usr/bin/env python3
"""Repackage our freestanding fixture for an independent qemu-aarch64 baseline.

Guest virtual addresses and computational instructions stay unchanged. Only the
three fixed assembly syscall instructions (MOV number/target register, SVC) are
adapted from Darwin to Linux. This does not convert arbitrary Mach-O programs.
"""
import argparse
import struct
from pathlib import Path


def convert(source):
    magic,cpu,subtype,kind,commands,command_bytes,flags,reserved=struct.unpack_from('<8I',source)
    assert magic==0xfeedfacf and cpu==0x100000c and kind==2
    segments=[];entry_offset=None;text=None;cursor=32
    for _ in range(commands):
        cmd,size=struct.unpack_from('<II',source,cursor)
        if cmd==0x19:
            address,memory_size,file_offset,file_size=struct.unpack_from('<4Q',source,cursor+24)
            protection=struct.unpack_from('<I',source,cursor+60)[0]
            sections=struct.unpack_from('<I',source,cursor+64)[0]
            if protection:
                segments.append([address,memory_size,file_offset,bytearray(source[file_offset:file_offset+file_size]),protection])
            for i in range(sections):
                s=cursor+72+i*80
                if source[s:s+16].rstrip(b'\0')==b'__text':text=struct.unpack_from('<QQ',source,s+32)
        elif cmd==0x80000028:entry_offset=struct.unpack_from('<Q',source,cursor+8)[0]
        cursor+=size
    assert text and entry_offset is not None
    entry=next(address+entry_offset-off for address,mem,off,data,prot in segments if prot&4 and off<=entry_offset<off+len(data))
    replacements={0xd2800090:0xd2800808,0xd2800030:0xd2800ba8,0xd4001001:0xd4000001}
    patched={hex(k):0 for k in replacements}
    for address,mem,off,data,prot in segments:
        if not prot&4:continue
        for at in range(text[0]-address,text[0]-address+text[1],4):
            word=struct.unpack_from('<I',data,at)[0]
            if word in replacements:struct.pack_into('<I',data,at,replacements[word]);patched[hex(word)]+=1
    assert patched['0xd2800030']==1 and patched['0xd4001001']==1+patched['0xd2800090'],patched
    header=struct.pack('<16sHHIQQQIHHHHHH',b'\x7fELF\x02\x01\x01'+b'\0'*9,2,183,1,entry,64,0,0,64,56,len(segments),0,0,0)
    output=bytearray(header+b'\0'*(56*len(segments)))
    for index,(address,memory_size,off,data,protection) in enumerate(segments):
        offset=(len(output)+4095)&~4095;output.extend(b'\0'*(offset-len(output)))
        elf_flags=(4 if protection&1 else 0)|(2 if protection&2 else 0)|(1 if protection&4 else 0)
        struct.pack_into('<II6Q',output,64+56*index,1,elf_flags,offset,address,address,len(data),memory_size,4096)
        output.extend(data)
    return output,patched


if __name__=='__main__':
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('source',type=Path);p.add_argument('output',type=Path);o=p.parse_args()
    data,patched=convert(o.source.read_bytes());o.output.write_bytes(data);o.output.chmod(0o755);print(patched)
