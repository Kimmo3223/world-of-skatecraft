#!/usr/bin/env python3
"""Unpack a Skate 3 "EB" audio archive (data/audio/*.big): a table of (offset >> shift, size,
name hash) entries, then NUL-separated names, then the files. The shift is header byte 0x0a."""
import struct, sys, os

def unpack(path, out):
    d = open(path, 'rb').read()
    assert d[:2] == b'EB', path
    count = struct.unpack('>I', d[4:8])[0]
    shift = d[0x0a]
    names_at = struct.unpack('>I', d[0x0c:0x10])[0]
    entries = [struct.unpack('>IIII', d[0x30 + i * 16:0x40 + i * 16]) for i in range(count)]
    # Names: each preceded by two zero bytes, NUL-terminated.
    names, p = [], names_at
    while len(names) < count:
        while d[p] == 0:
            p += 1
        e = d.index(b'\0', p)
        names.append(d[p:e].decode('latin1'))
        p = e
    os.makedirs(out, exist_ok=True)
    for (off, _, size, _), name in zip(entries, names):
        open(os.path.join(out, name), 'wb').write(d[(off << shift):(off << shift) + size])
    return names

if __name__ == '__main__':
    for n in unpack(sys.argv[1], sys.argv[2]):
        print(n)
