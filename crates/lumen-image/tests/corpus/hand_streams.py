import zlib, struct
class B:
    def __init__(s): s.bits=[]
    def put(s,v,n): s.bits += [(v>>i)&1 for i in range(n)]
    def huff(s,c,n): s.bits += [(c>>i)&1 for i in range(n-1,-1,-1)]
    def done(s):
        b=s.bits+[0]*(-len(s.bits)%8)
        print(''.join(map(str,s.bits)), len(s.bits))
        return bytes(sum(b[i+j]<<j for j in range(8)) for i in range(0,len(b),8))
def wrap(d,raw): return b'\x78\x01'+d+struct.pack('>I',zlib.adler32(raw))
# fixed
b=B(); b.put(1,1); b.put(1,2); b.huff(0x30+97,8); b.huff(1,7); b.huff(0,5); b.huff(0,7)
f=wrap(b.done(),b'aaaa'); print(f.hex(), zlib.decompress(f))
# dynamic
b=B(); b.put(1,1); b.put(2,2); b.put(0,5); b.put(0,5); b.put(14,4)
order=[16,17,18,0,8,7,9,6,10,5,11,4,12,3,13,2,14,1,15]
cl={18:1,1:1}
for s in order[:18]: b.put(cl.get(s,0),3)
# code-length code: symbol 1 -> '0', symbol 18 -> '1'
b.huff(1,1); b.put(86,7)   # 97 zeros
b.huff(0,1)                # 'a' length 1
b.huff(1,1); b.put(127,7)  # 138 zeros
b.huff(1,1); b.put(9,7)    # 20 zeros
b.huff(0,1)                # 256 length 1
b.huff(0,1)                # distance 0 length 1
b.huff(0,1); b.huff(0,1); b.huff(1,1)  # a a EOB
d=wrap(b.done(),b'aa'); print(d.hex(), zlib.decompress(d))
