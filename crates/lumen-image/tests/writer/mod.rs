//! A deflate bit writer for the tests: it assembles streams bit by bit from RFC 1951 and RFC
//! 1950 as written, sharing no code with the decoder, so a test's stream and its expected
//! offsets come from the RFCs and not from what the decoder reads.

/// Bits packed least significant first into bytes (RFC 1951 section 3.1.1).
#[derive(Default)]
pub struct Writer {
    bits: Vec<u8>,
}

impl Writer {
    /// A stream that starts with the zlib header `cmf`, `flg`.
    pub fn zlib(cmf: u8, flg: u8) -> Self {
        let mut w = Self::default();
        w.value(u32::from(cmf), 8);
        w.value(u32::from(flg), 8);
        w
    }

    /// The default header: deflate, a 32 KiB window, no dictionary (`78 01`).
    pub fn deflate() -> Self {
        Self::zlib(0x78, 0x01)
    }

    /// `count` bits of `value`, least significant first: header fields and extra bits.
    pub fn value(&mut self, value: u32, count: u32) -> &mut Self {
        self.bits.extend((0..count).map(|i| u8::try_from((value >> i) & 1).unwrap()));
        self
    }

    /// A Huffman code of `length` bits, most significant first (section 3.1.1).
    pub fn code(&mut self, code: u32, length: u32) -> &mut Self {
        self.bits.extend((0..length).rev().map(|i| u8::try_from((code >> i) & 1).unwrap()));
        self
    }

    /// A block header: `BFINAL`, then `BTYPE` in two bits.
    pub fn block(&mut self, last: bool, kind: u32) -> &mut Self {
        self.value(u32::from(last), 1).value(kind, 2)
    }

    /// Zero bits to the next byte boundary.
    pub fn align(&mut self) -> &mut Self {
        while self.bits.len() % 8 != 0 {
            self.bits.push(0);
        }
        self
    }

    /// Whole bytes, after [`Self::align`].
    pub fn bytes(&mut self, bytes: &[u8]) -> &mut Self {
        for &b in bytes {
            self.value(u32::from(b), 8);
        }
        self
    }

    /// The offset of the byte that holds the last bit written so far: where a decoder that
    /// faults on that bit must report the fault.
    pub fn last_byte(&self) -> usize {
        (self.bits.len() - 1) / 8
    }

    /// The bits so far, zero-padded to a whole byte.
    pub fn finish(&self) -> Vec<u8> {
        self.bits
            .chunks(8)
            .map(|byte| byte.iter().enumerate().map(|(i, &b)| b << i).sum())
            .collect()
    }

    /// The bits so far, aligned, then the Adler-32 trailer of `decoded`: a complete stream.
    pub fn trailer(&mut self, decoded: &[u8]) -> Vec<u8> {
        self.align();
        self.bytes(&adler32(decoded).to_be_bytes());
        self.finish()
    }

    /// A fixed-Huffman literal (section 3.2.6): 0-143 as `00110000` plus the byte in 8 bits,
    /// 144-255 as `110010000` plus the byte less 144 in 9 bits.
    pub fn fixed_literal(&mut self, byte: u8) -> &mut Self {
        if byte < 144 {
            self.code(0x30 + u32::from(byte), 8)
        } else {
            self.code(0x190 + u32::from(byte) - 144, 9)
        }
    }
}

/// The second header byte that makes `cmf` and `flags` (the top three bits, `FDICT` and
/// `FLEVEL`) a multiple of 31 (RFC 1950 section 2.2, `FCHECK`).
pub fn flg_for(cmf: u8, flags: u8) -> u8 {
    let base = u32::from(cmf) * 256 + u32::from(flags & 0xe0);
    flags & 0xe0 | u8::try_from((31 - base % 31) % 31).unwrap()
}

/// Adler-32 straight from its definition (RFC 1950 section 8): A is one plus the sum of the
/// bytes and B the sum of each step's A, both modulo 65 521, reduced at every byte.
pub fn adler32(bytes: &[u8]) -> u32 {
    let (mut a, mut b) = (1_u32, 0_u32);
    for &byte in bytes {
        a = (a + u32::from(byte)) % 65_521;
        b = (b + a) % 65_521;
    }
    b * 65_536 + a
}
