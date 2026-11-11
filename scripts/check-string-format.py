#!/usr/bin/env python3
"""Independently decode emitted original constant strings, provider graph and getter."""
import argparse,hashlib,json,runpy,struct
from pathlib import Path
method_format=runpy.run_path(str(Path(__file__).with_name('check-objc-format.py')))['inspect']
CF='/System/Library/Frameworks/CoreFoundation.framework/CoreFoundation'
FOUNDATION='/System/Library/Frameworks/Foundation.framework/Foundation'
def inspect(path,expected):
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
    deps=[];exports={};at=32
    for _ in range(u32(16)):
        command,size=struct.unpack_from('<II',raw,at)
        if command==0xc:deps.append(raw[at+u32(at+8):at+size].split(b'\0')[0].decode())
        if command==2:
            symoff,count,stroff,_=struct.unpack_from('<4I',raw,at+8)
            for i in range(count):
                item=symoff+i*16
                if raw[item+4]&15==15:
                    start=stroff+u32(item);exports[raw[start:raw.index(0,start)].decode()]=u64(item+8)
        at+=size
    result=dict(file=path.name,sha256=hashlib.sha256(raw).hexdigest(),dependencies=deps,pointer_formats=sorted(formats))
    rows=[]
    if '__cfstring' in sections:
        s=sections['__cfstring'];assert s['align']==3 and s['length']%32==0 and s['flags']&255==0
        for n in range(0,s['length'],32):
            a=s['address']+n;assert pointer(a)=='___CFConstantStringClassReference'
            flags,pad=read(a+8,'<II');count=read(a+24,'<Q')[0];assert flags==0x7c8 and pad==0 and count<=63
            value=string(pointer(a+16));assert len(value)==count and all(0<ord(c)<128 for c in value)
            rows.append(dict(address=a,flags=flags,length=count,bytes=value))
        assert CF in deps
    result['constants']=rows
    if path.name=='libsable_literals.dylib':
        assert '/usr/lib/libobjc.A.dylib' not in deps and {r['bytes'] for r in rows}=={'sable foundation','dylib Alpha09'}
    if path.name.startswith('strings-'):
        assert {r['length'] for r in rows}=={0,16,63}
        assert {'_objc_msgSend','_strcmp','_strlen','_write'}.issubset(imports)
        assert not any('autorelease' in s.lower() or 'retain' in s.lower() for s in imports)
    if path.name in ['Foundation.dylib','CoreFoundation.dylib']:
        cl=sections['__objc_classlist'];assert cl['length']==8
        c=pointer(cl['address']);meta=pointer(c);ro=pointer(c+32);mr=pointer(meta+32)
        root=path.name=='Foundation.dylib';assert read(ro,'<4I')==((2,0,32,0) if root else (0,32,32,0))
        assert read(mr,'<4I')==((3,40,40,0) if root else (1,40,40,0))
        if root:
            assert string(pointer(ro+24))=='NSString' and pointer(c+8)==0
            ivars=pointer(ro+48);assert read(ivars,'<II')==(32,4)
            assert [read(pointer(ivars+8+n*32),'<I')[0] for n in range(4)]==[0,8,16,24]
            methods=pointer(ro+32);flags,count=read(methods,'<II');assert count==1
            row=methods+8
            if flags==24:sel,ty,imp=pointer(row),pointer(row+8),pointer(row+16)
            else:
                assert flags==0x8000000c
                off=read(row,'<iii');sel=pointer(row+off[0]);ty=row+4+off[1];imp=row+8+off[2]
            assert string(sel)=='UTF8String' and string(ty)=='r*16@0:8'
            assert sections['__text']['address']<=imp<sections['__text']['address']+sections['__text']['length']
            result['getter']=dict(selector=string(sel),encoding=string(ty),imp=imp)
            result['method_format']=method_format(path,expected)
        else:
            assert pointer(c+8)=='_OBJC_CLASS_$_NSString' and FOUNDATION in deps
            assert exports['___CFConstantStringClassReference']==c
            assert pointer(ro+48)==0
            result['class_alias_address']=c
        result['class_layout']=dict(start=read(ro+4,'<I')[0],size=read(ro+8,'<I')[0])
    return result

def main():
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('format',choices=['absolute','compact']);p.add_argument('directory',type=Path);o=p.parse_args()
    names=['strings-0.macho','strings-1.macho','libsable_literals.dylib','Foundation.dylib','CoreFoundation.dylib']
    print(json.dumps([inspect(o.directory/n,o.format) for n in names],indent=2))
if __name__=='__main__':main()
