#!/usr/bin/env python3
"""Assert the emitted original fixture format; no guest execution or Apple tools."""
import argparse
import hashlib
import json
from pathlib import Path
import struct


def inspect(path, expected):
    raw = path.read_bytes()
    sections = {}
    at = 32
    for _ in range(struct.unpack_from('<I', raw, 16)[0]):
        command, size = struct.unpack_from('<II', raw, at)
        if command == 0x19:
            protection = struct.unpack_from('<I', raw, at+60)[0]
            for i in range(struct.unpack_from('<I', raw, at+64)[0]):
                offset = at+72+i*80
                name = raw[offset:offset+16].split(b'\0')[0].decode()
                address, length, file_offset = struct.unpack_from('<QQI', raw, offset+32)
                sections[name] = dict(address=address, length=length, offset=file_offset,
                                      protection=protection, flags=struct.unpack_from('<I',raw,offset+64)[0])
        at += size
    result = dict(file=path.name, sha256=hashlib.sha256(raw).hexdigest(), format=expected, lists=[])
    if expected == 'absolute':
        assert '__objc_methlist' not in sections, path
        return result
    section = sections['__objc_methlist']
    assert section['protection'] == 5 and section['flags'] & 0x800004ff == 0
    assert section['address'] % 4 == 0 and section['length'] % 4 == 0
    result['section'] = section
    end = section['offset'] + section['length']
    at = section['offset']
    def contains(name, address, length):
        s = sections[name]
        return s['address'] <= address and address+length <= s['address']+s['length']
    while at < end:
        header = struct.unpack_from('<I', raw, at)[0]
        if header == 0:  # Linker padding between complete lists.
            at += 4
            continue
        header, count = struct.unpack_from('<II', raw, at)
        assert header == 0x8000000c and 0 < count <= 128 and at+8+count*12 <= end
        entries = []
        for n in range(count):
            file_at = at+8+n*12
            entry = section['address']+file_at-section['offset']
            offsets = struct.unpack_from('<iii', raw, file_at)
            slot, types, imp = [entry+i*4+offsets[i] for i in range(3)]
            assert slot % 8 == 0 and contains('__objc_selrefs',slot,8)
            assert any(contains(name,types,1) for name in ['__objc_methtype','__cstring'] if name in sections)
            assert imp % 4 == 0 and contains('__text',imp,4)
            entries.append(dict(entry=entry, offsets=offsets, selector_slot=slot, types=types, imp=imp))
        result['lists'].append(dict(address=section['address']+at-section['offset'], header=header, count=count, entries=entries))
        at += 8+count*12
    assert result['lists'] and any(e['offsets'][2] < 0 for m in result['lists'] for e in m['entries'])
    return result


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('format', choices=['absolute','compact'])
    p.add_argument('directory', type=Path)
    o = p.parse_args()
    files = [o.directory/f'objc-{n}.macho' for n in range(3)] + [o.directory/'libsable_other.dylib']
    print(json.dumps([inspect(path,o.format) for path in files], indent=2))


if __name__ == '__main__':
    main()
