#!/usr/bin/env python3
"""Inspect original linked protocol fixtures without Apple tools or guest execution."""
import argparse
import hashlib
import json
from pathlib import Path
import struct


def inspect(path):
    raw=path.read_bytes();sections={};segments=[];chain=None;at=32
    def u32(at):return struct.unpack_from('<I',raw,at)[0]
    def u64(at):return struct.unpack_from('<Q',raw,at)[0]
    for _ in range(u32(16)):
        command,size=struct.unpack_from('<II',raw,at)
        if command==0x19:
            segments.append(dict(address=u64(at+24),length=u64(at+32),offset=u64(at+40),file_size=u64(at+48)))
            for i in range(u32(at+64)):
                s=at+72+i*80;name=raw[s:s+16].split(b'\0')[0].decode()
                address,length,offset=struct.unpack_from('<QQI',raw,s+32)
                sections[name]=dict(address=address,length=length,offset=offset,flags=u32(s+64),protection=u32(at+60))
        elif command==0x80000034:chain=u32(at+8)
        at+=size
    formats=set()
    if chain is not None:
        start=chain+u32(chain+4)
        for i in range(u32(start)):
            offset=u32(start+4+i*4)
            if offset:formats.add(struct.unpack_from('<H',raw,start+offset+6)[0])
        assert formats and formats.issubset({2,6}) and len(formats)==1
    base=next(s['address'] for s in segments if s['offset']==0 and s['file_size'])
    def read(address,fmt):
        size=struct.calcsize(fmt)
        s=next(s for s in sections.values() if s['address']<=address and address+size<=s['address']+s['length'])
        assert s['flags']&255!=1, 'Expected file-backed metadata'
        return struct.unpack_from(fmt,raw,s['offset']+address-s['address'])
    def pointer(address):
        value=read(address,'<Q')[0]
        if not value or not formats:return value
        assert value>>63==0, 'Own metadata must not bind to an external definition'
        target=(value&((1<<36)-1))|(((value>>36)&255)<<56)
        return target+(base if formats=={6} else 0)
    def name(address):
        out=bytearray()
        for i in range(256):
            b=read(address+i,'<B')[0]
            if not b:return out.decode()
            out.append(b)
        raise AssertionError('unterminated fixture name')
    listed=sections['__objc_protolist'];refs=sections['__objc_protorefs'];data=sections['__data']
    assert listed['flags']&255 in [0,11] and refs['flags']&255==11
    assert listed['length']==16 and listed['address']%8==refs['address']%8==0
    protocols=[];known={}
    for offset in range(0,listed['length'],8):
        p=pointer(listed['address']+offset)
        assert p%8==0 and data['address']<=p and p+96<=data['address']+data['length']
        assert data['protection']==3 and read(p+64,'<II')==(96,0)
        assert all(read(p+n,'<Q')[0]==0 for n in [0,16,32,40,48,56,80,88])
        label=name(pointer(p+8));descs=pointer(p+24);extended=pointer(p+72)
        assert read(descs,'<II')==(24,1)
        selector=name(pointer(descs+8));types=pointer(descs+16)
        assert name(types)=='Q24@0:8Q16' and read(descs+24,'<Q')[0]==0
        assert pointer(extended)==types
        assert label not in known;known[label]=p
        protocols.append(dict(name=label,address=p,name_address=pointer(p+8),size=96,description_selector=selector,description_types=name(types),description_imp=0))
    assert set(known)=={'SableScalar','SableOtherScalar'}
    for offset in range(0,refs['length'],8):assert pointer(refs['address']+offset) in known.values()
    classes={};cl=sections['__objc_classlist']
    for offset in range(0,cl['length'],8):
        p=pointer(cl['address']+offset);ro=pointer(p+32);meta_ro=pointer(pointer(p)+32)
        adoption=pointer(ro+40);assert pointer(meta_ro+40)==adoption
        adopted=[]
        if adoption:
            count=read(adoption,'<Q')[0];assert count<=64
            for i in range(count):
                target=pointer(adoption+8+i*8);adopted.append(next(k for k,v in known.items() if v==target))
        classes[name(pointer(ro+24))]=adopted
    assert classes=={'ProtocolRoot':['SableScalar'],'UnrelatedRoot':[],'DeclaredOnlyRoot':['SableScalar']}
    return dict(file=path.name,sha256=hashlib.sha256(raw).hexdigest(),pointer_formats=sorted(formats),protocols=protocols,classes=classes,data_section=data)


def main():
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('directory',type=Path);o=p.parse_args()
    print(json.dumps([inspect(o.directory/f'protocol-{n}.macho') for n in [0,1]],indent=2))


if __name__=='__main__':main()
