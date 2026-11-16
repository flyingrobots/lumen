"""Builds the zlib corpus of lumen-image's inflate tests, outside the repository's build.

Run as `python3 -I make_corpus.py OUT_DIR` with CPython's standard library only. Each case
writes OUT_DIR/<name>.raw (the encoder's input, which is the expected output of decoding)
and OUT_DIR/<name>.zlib (the stream). Streams come from CPython's `zlib` module, except where
zlib cannot produce the shape under test: there a small bit writer below assembles the deflate
blocks, and the stream is checked to decompress to the raw bytes with `zlib.decompress` before
it is written. The script prints, for each stream, its raw file, sizes, block types (read back by `block_types`)
and SHA-256, then each raw file's SHA-256: the tables in the corpus README.
"""

import hashlib
import random
import struct
import sys
import zlib

OUT = sys.argv[1]


def text(seed, size):
    """Deterministic pseudo-English: words from a fixed list, chosen by a seeded generator."""
    words = ("the light falls on a bunny made of integer triangles while every ray "
             "returns the same bytes on each platform and thread count lumen renders "
             "texture maps decoded without floating point").split()
    rng = random.Random(seed)
    out = bytearray()
    while len(out) < size:
        out += rng.choice(words).encode() + (b"\n" if rng.random() < 0.08 else b" ")
    return bytes(out[:size])


def noise(seed, size):
    rng = random.Random(seed)
    return bytes(rng.getrandbits(8) for _ in range(size))


def deflate(raw, level=6, wbits=15, strategy=zlib.Z_DEFAULT_STRATEGY, mem=8):
    c = zlib.compressobj(level, zlib.DEFLATED, wbits, mem, strategy)
    return c.compress(raw) + c.flush()


class Bits:
    """An LSB-first bit writer (RFC 1951 section 3.1.1)."""

    def __init__(self):
        self.out, self.acc, self.n = bytearray(), 0, 0

    def put(self, value, count):
        self.acc |= value << self.n
        self.n += count
        while self.n >= 8:
            self.out.append(self.acc & 0xFF)
            self.acc >>= 8
            self.n -= 8

    def huff(self, code, length):
        """A Huffman code, most significant bit first (RFC 1951 section 3.1.1)."""
        for i in range(length - 1, -1, -1):
            self.put((code >> i) & 1, 1)

    def align(self):
        if self.n:
            self.put(0, 8 - self.n)
        return bytes(self.out)


def fixed_literal(bits, byte):
    if byte < 144:
        bits.huff(0x30 + byte, 8)
    else:
        bits.huff(0x190 + byte - 144, 9)


def zlib_wrap(raw_deflate, raw):
    return b"\x78\x01" + raw_deflate + struct.pack(">I", zlib.adler32(raw))


def max_distance():
    """32 768 literals, then one length-258 match at distance 32 768, in one fixed block.

    zlib's encoder never emits a distance beyond 32 506 (MAX_DIST), so this is assembled.
    Length 258 is symbol 285 (8-bit code 11000101, no extra bits); distance 32 768 is code
    29 (5-bit code 11101) with 13 extra bits of 8 191 (base 24 577).
    """
    head = noise(11, 32768)
    bits = Bits()
    bits.put(1, 1)
    bits.put(1, 2)
    for b in head:
        fixed_literal(bits, b)
    bits.huff(0b11000101, 8)
    bits.huff(29, 5)
    bits.put(8191, 13)
    bits.huff(0, 7)
    raw = head + head[:258]
    return raw, zlib_wrap(bits.align(), raw)


def final_stored():
    """A dynamic or fixed block from zlib, a sync flush (an empty stored block), then a final
    stored block assembled by hand (BFINAL 1, BTYPE 00, LEN, NLEN, the bytes)."""
    a, b = text(21, 3000), text(22, 700)
    c = zlib.compressobj(6, zlib.DEFLATED, -15)
    body = c.compress(a) + c.flush(zlib.Z_SYNC_FLUSH)
    body += b"\x01" + struct.pack("<HH", len(b), len(b) ^ 0xFFFF) + b
    raw = a + b
    return raw, zlib_wrap(body, raw)


CASES = {
    "empty": (b"", lambda r: deflate(r)),
    "one_byte": (b"L", lambda r: deflate(r)),
    "stored_small": (text(1, 1000), lambda r: deflate(r, level=0)),
    "stored_multi": (text(2, 70000), lambda r: deflate(r, level=0)),
    "short_text": (text(3, 120), lambda r: deflate(r)),
    "fixed_strategy": (text(4, 20000), lambda r: deflate(r, strategy=zlib.Z_FIXED)),
    "huffman_only": (text(5, 20000), lambda r: deflate(r, strategy=zlib.Z_HUFFMAN_ONLY)),
    "rle": (text(6, 20000), lambda r: deflate(r, strategy=zlib.Z_RLE)),
    "noise": (noise(7, 20000), lambda r: deflate(r)),
    "run_258": (b"a" * 20000, lambda r: deflate(r, level=9)),
    "small_window": (text(8, 20000), lambda r: deflate(r, wbits=9)),
}
for level in range(1, 10):
    CASES[f"level_{level}"] = (text(100, 40000), (lambda lv: lambda r: deflate(r, level=lv))(level))


def block_types(stream):
    """The type of each block, in order, read by a minimal walker of RFC 1951 written for this
    record (it decodes Huffman codes canonically, bit by bit, like zlib's contrib/puff)."""
    data, pos = stream[2:-4], [0]

    def bit():
        b = (data[pos[0] >> 3] >> (pos[0] & 7)) & 1
        pos[0] += 1
        return b

    def bits(n):
        return sum(bit() << i for i in range(n))

    def table(lengths):
        codes, code = {}, 0
        for length in range(1, 16):
            for sym, l in enumerate(lengths):
                if l == length:
                    codes[(length, code)] = sym
                    code += 1
            code <<= 1
        return codes

    def decode(codes):
        code, length = 0, 0
        while True:
            code, length = (code << 1) | bit(), length + 1
            if (length, code) in codes:
                return codes[(length, code)]

    lbase = [3, 4, 5, 6, 7, 8, 9, 10, 11, 13, 15, 17, 19, 23, 27, 31, 35, 43, 51, 59, 67, 83,
             99, 115, 131, 163, 195, 227, 258]
    lext = [0] * 8 + [i // 4 for i in range(4, 24)] + [0]
    dext = [0, 0] + [i // 2 - 1 for i in range(2, 30)]
    kinds = []
    while True:
        final, kind = bit(), bits(2)
        kinds.append(["stored", "fixed", "dynamic"][kind])
        if kind == 0:
            pos[0] = (pos[0] + 7) & ~7
            n = bits(16)
            bits(16)
            pos[0] += 8 * n
        else:
            if kind == 1:
                lit = table([8] * 144 + [9] * 112 + [7] * 24 + [8] * 8)
                dist = table([5] * 30)
            else:
                hlit, hdist, hclen = bits(5) + 257, bits(5) + 1, bits(4) + 4
                order = [16, 17, 18, 0, 8, 7, 9, 6, 10, 5, 11, 4, 12, 3, 13, 2, 14, 1, 15]
                cl = [0] * 19
                for i in range(hclen):
                    cl[order[i]] = bits(3)
                clt, lengths = table(cl), []
                while len(lengths) < hlit + hdist:
                    s = decode(clt)
                    if s < 16:
                        lengths.append(s)
                    elif s == 16:
                        lengths += [lengths[-1]] * (3 + bits(2))
                    elif s == 17:
                        lengths += [0] * (3 + bits(3))
                    else:
                        lengths += [0] * (11 + bits(7))
                lit, dist = table(lengths[:hlit]), table(lengths[hlit:])
            while True:
                s = decode(lit)
                if s == 256:
                    break
                if s > 256:
                    bits(lext[s - 257])
                    bits(dext[decode(dist)])
        if final:
            return kinds


def summary(kinds):
    """Runs of equal block types: `stored x2, fixed`."""
    out = []
    for k in kinds:
        if out and out[-1][0] == k:
            out[-1][1] += 1
        else:
            out.append([k, 1])
    return ", ".join(k if n == 1 else f"{k} x{n}" for k, n in out)


def main():
    made = {name: (raw, make(raw)) for name, (raw, make) in CASES.items()}
    made["max_distance"] = max_distance()
    made["final_stored"] = final_stored()
    raws = {}
    for name, (raw, stream) in sorted(made.items()):
        assert zlib.decompress(stream) == raw, name
        # Equal inputs share one raw file, named for the first case that uses it.
        raw_name = raws.setdefault(raw, name)
        files = [(f"{name}.zlib", stream)]
        if raw_name == name:
            files.append((f"{name}.raw", raw))
        for file, data in files:
            with open(f"{OUT}/{file}", "wb") as f:
                f.write(data)
        print(f"| `{name}` | `{raw_name}.raw` | {len(raw)} | {len(stream)} | "
              f"{summary(block_types(stream))} | `{hashlib.sha256(stream).hexdigest()}` |")
    for raw, name in sorted(raws.items(), key=lambda kv: kv[1]):
        print(f"| `{name}.raw` | {len(raw)} | `{hashlib.sha256(raw).hexdigest()}` |")


main()
