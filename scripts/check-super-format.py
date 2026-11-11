#!/usr/bin/env python3
"""Independently inspect original hierarchy/fixup records; no guest execution."""
import argparse
import hashlib
import json
from pathlib import Path
import runpy
import struct

method_format=runpy.run_path(str(Path(__file__).with_name('check-objc-format.py')))['inspect']


def inspect(path, expected, split, class_names=("SableBase","SableChild","SableGrandchild"), pointer_signatures=False, payload_sizes=None):
    base_name,child_name,deep_name=class_names
    raw=path.read_bytes();sections={};segments=[];binds={};imports=[];chain=None;bind_stream=None;at=32
    def u32(at):return struct.unpack_from('<I',raw,at)[0]
    def u64(at):return struct.unpack_from('<Q',raw,at)[0]
    for _ in range(u32(16)):
        command,size=struct.unpack_from('<II',raw,at)
        if command==0x19:
            segments.append(dict(address=u64(at+24),offset=u64(at+40),file_size=u64(at+48)))
            for i in range(u32(at+64)):
                s=at+72+i*80;name=raw[s:s+16].split(b'\0')[0].decode()
                address,length,offset=struct.unpack_from('<QQI',raw,s+32)
                sections[name]=dict(address=address,length=length,offset=offset,align=u32(s+52),flags=u32(s+64),protection=u32(at+60))
        elif command==0x80000034:chain=u32(at+8)
        elif command==0x80000022:bind_stream=(u32(at+16),u32(at+20))
        elif command==2:
            # Legacy function references may use lazy binds. Undefined nlist
            # symbols independently retain their imported names.
            symoff,count,stroff,_=struct.unpack_from('<4I',raw,at+8)
            for i in range(count):
                item=symoff+i*16
                if raw[item+4]&15==1:
                    start=stroff+u32(item)
                    imports.append(raw[start:raw.index(0,start)].decode())
        at+=size
    def read(address,fmt):
        length=struct.calcsize(fmt)
        s=next(s for s in sections.values() if s['address']<=address and address+length<=s['address']+s['length'])
        return struct.unpack_from(fmt,raw,s['offset']+address-s['address'])
    # Legacy binds identify foreign superclass/metaclass fields. Restrict this
    # independent decoder to emitted non-lazy pointer binds with zero addends.
    if bind_stream:
        pos,end=bind_stream[0],sum(bind_stream);segment=offset=0;symbol=None
        def leb(signed=False):
            nonlocal pos
            value=shift=0
            while True:
                byte=raw[pos];pos+=1;value|=(byte&127)<<shift;shift+=7
                if not byte&128:break
            return value-(1<<shift) if signed and byte&64 else value
        def bind():
            nonlocal offset
            assert symbol is not None
            binds[segments[segment]['address']+offset]=symbol;offset+=8
        while pos<end:
            byte=raw[pos];pos+=1;op=byte&240;imm=byte&15
            if op==0:break
            if op in [0x10,0x30]:pass
            elif op==0x20:leb()
            elif op==0x40:
                stop=raw.index(0,pos);symbol=raw[pos:stop].decode();pos=stop+1;imports.append(symbol)
            elif op==0x50:assert imm==1
            elif op==0x60:assert leb(True)==0
            elif op==0x70:segment=imm;offset=leb()
            elif op==0x80:offset=(offset+leb())&((1<<64)-1)
            elif op==0x90:bind()
            elif op==0xa0:bind();offset=(offset+leb())&((1<<64)-1)
            elif op==0xb0:bind();offset+=imm*8
            elif op==0xc0:
                count,skip=leb(),leb()
                for _ in range(count):bind();offset=(offset+skip)&((1<<64)-1)
            else:raise AssertionError(hex(byte))
    formats=set();chain_imports=[]
    if chain is not None:
        start=chain+u32(chain+4)
        for i in range(u32(start)):
            off=u32(start+4+i*4)
            if off:formats.add(struct.unpack_from('<H',raw,start+off+6)[0])
        assert formats and len(formats)==1 and formats.issubset({2,6})
        imp=chain+u32(chain+8);symbols=chain+u32(chain+12);fmt=u32(chain+20);assert fmt in [1,2]
        for n in range(u32(chain+16)):
            item=u32(imp+n*(4 if fmt==1 else 8));name_at=symbols+(item>>9)
            symbol=raw[name_at:raw.index(0,name_at)].decode();chain_imports.append(symbol);imports.append(symbol)
    base=next(s['address'] for s in segments if s['offset']==0 and s['file_size'])
    def pointer(address):
        if address in binds:return binds[address]
        word=read(address,'<Q')[0]
        if word==0 or not formats:return word
        if word>>63:return chain_imports[word&0xffffff]
        target=(word&((1<<36)-1))|(((word>>36)&255)<<56)
        return target+(base if formats=={6} else 0)
    # Read bounded strings one byte at a time: a short section need not have
    # 256 readable bytes after its last string.
    def string(address):
        result=bytearray()
        for i in range(256):
            b=read(address+i,'<B')[0]
            if not b:return result.decode()
            result.append(b)
        raise AssertionError('unterminated fixture string')
    classes={};payload_layouts={};cl=sections['__objc_classlist'];is_base=path.suffix=='.dylib'
    root_size,child_size=payload_sizes or (8,8)
    for offset in range(0,cl['length'],8):
        p=pointer(cl['address']+offset);meta=pointer(p);ro=pointer(p+32);mr=pointer(meta+32)
        name=string(pointer(ro+24));root=name==base_name
        assert name in class_names
        assert read(ro,'<4I')==(2 if root else 0,0 if root else root_size,root_size if root else child_size,0)
        assert read(mr,'<4I')==(3 if root else 1,40,40,0)
        assert string(pointer(mr+24))==name
        if payload_sizes is None:
            if not root:assert pointer(ro+48)==0
        else:
            ivars=pointer(ro+48);fields=[]
            expected_fields=[]
            if root:expected_fields.append(('isa','#',0))
            start=8 if root else root_size
            size=root_size if root else child_size
            for n in range((size-start)//8):
                field=('baseValue' if n==0 else f'base{n+1}') if root else ('childValue' if n==0 else f'child{n+1}')
                expected_fields.append((field,'Q',start+n*8))
            if expected_fields:
                assert read(ivars,'<II')==(32,len(expected_fields))
                for n in range(len(expected_fields)):
                    row=ivars+8+n*32;cell=pointer(row)
                    assert cell%4==0 and read(row+24,'<II')==(3,8)
                    fields.append((string(pointer(row+8)),string(pointer(row+16)),read(cell,'<I')[0]))
                assert fields==expected_fields
            else:assert ivars==0
            payload_layouts[name]=dict(start=0 if root else root_size,size=size,fields=fields)
        classes[name]=dict(address=p,meta=meta,superclass=pointer(p+8),meta_isa=pointer(meta),meta_superclass=pointer(meta+8))
    expected_names={base_name} if is_base else ({child_name} if split else {base_name,child_name})
    if path.stem.endswith('-2'):expected_names.add(deep_name)
    assert set(classes)==expected_names
    if base_name in classes:
        b=classes[base_name];assert b['superclass']==0 and b['meta_isa']==b['meta'] and b['meta_superclass']==b['address']
    if child_name in classes:
        c=classes[child_name]
        parent='_OBJC_CLASS_$_'+base_name if split else classes[base_name]['address']
        parent_meta='_OBJC_METACLASS_$_'+base_name if split else classes[base_name]['meta']
        assert c['superclass']==parent and c['meta_isa']==c['meta_superclass']==parent_meta, (path, c, parent, parent_meta)
        refs=sections['__objc_superrefs'];assert refs['length']==(8 if pointer_signatures or payload_sizes else 16) and refs['align']==3 and refs['flags']&255==0
        targets=[pointer(refs['address']+i) for i in range(0,refs['length'],8)]
        assert set(targets)==({c['address']} if pointer_signatures or payload_sizes else {c['address'],c['meta']})
        assert '_objc_msgSendSuper2' in imports
    else:targets=[]
    if deep_name in classes:
        g=classes[deep_name];c=classes[child_name];assert g['superclass']==c['address'] and g['meta_superclass']==c['meta']
    result=dict(file=path.name,sha256=hashlib.sha256(raw).hexdigest(),classes=classes,superref_targets=targets,pointer_formats=sorted(formats),method_format=method_format(path,expected))
    if pointer_signatures or payload_sizes:
        methods=[]
        for label,cls in classes.items():
            for kind,address in [('instance',cls['address']),('class',cls['meta'])]:
                table=pointer(pointer(address+32)+32)
                if not table:continue
                flags,count=read(table,'<II');assert flags in [24,0x8000000c]
                stride=12 if flags>>31 else 24
                for n in range(count):
                    row=table+8+n*stride
                    if stride==24:sel,types=pointer(row),pointer(row+8)
                    else:
                        off=read(row,'<iii');sel=pointer(row+off[0]);types=row+4+off[1]
                    method=string(sel);encoding=string(types)
                    expected_type=({'base':'Q16@0:8','child':'Q16@0:8','sum':'Q16@0:8','setBase:':'v24@0:8Q16','setChild:':'v24@0:8Q16'} if payload_sizes else {'make:':'@24@0:8Q16','identity':'@16@0:8','choose:':'@24@0:8@16','matches:':'Q24@0:8@16','value:':'Q24@0:8Q16'})[method]
                    assert encoding==expected_type
                    methods.append(dict(owner=label,kind=kind,name=method,encoding=encoding))
        if payload_sizes:
            expected_methods=set()
            for label in classes:
                names=['base','sum']+(['setBase:'] if root_size>8 else []) if label==base_name else ['child','sum']+(['setChild:'] if child_size>root_size else [])
                expected_methods.update((label,'instance',n) for n in names)
            assert {(m['owner'],m['kind'],m['name']) for m in methods}==expected_methods
            result['payload_layouts']=payload_layouts
            if not is_base:assert '_class_createInstance' in imports and '_object_dispose' in imports
        else:
            assert len(methods)==(5 if is_base else 1 if split else 6)
            assert '_class_createInstance' in imports
            if not is_base:assert '_object_dispose' in imports
        result['methods']=methods
    return result


def main():
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('format',choices=['absolute','compact']);p.add_argument('directory',type=Path);p.add_argument('--split',action='store_true');o=p.parse_args()
    files=[o.directory/f'super-{n}.macho' for n in range(3)]
    if not o.split:files.append(o.directory/'super-3.macho')
    if o.split:files.append(o.directory/'libsable_base.dylib')
    print(json.dumps([inspect(f,o.format,o.split) for f in files],indent=2))


if __name__=='__main__':main()
