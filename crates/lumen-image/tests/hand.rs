//! Size class: small. Streams assembled bit by bit from RFC 1951 and RFC 1950, never from an
//! encoder: two written out in full in the comments (a fixed-Huffman and a dynamic-Huffman
//! block), then one stream per refusal, each with the offset the RFC's bit order puts it at.
//!
//! Oracle: the RFCs. The expected bytes are those the streams were built to hold, and each
//! expected offset is the byte that [`writer::Writer`] placed the faulting bit in.

#![allow(clippy::unwrap_used, reason = "test code fails loudly")]

mod writer;

use lumen_image::{inflate_zlib, Alphabet, InflateErrorKind, OutputLimit};
use writer::{adler32, flg_for, Writer};

const ROOM: OutputLimit = OutputLimit::new(1 << 20);

/// Asserts that `stream` is refused with `kind` at `offset`.
fn refused(stream: &[u8], kind: InflateErrorKind, offset: usize) {
    let error = inflate_zlib(stream, ROOM).unwrap_err();
    assert_eq!((error.kind(), error.offset()), (&kind, offset), "{error}");
}

/// The code-length code the dynamic streams below use, chosen to be complete: symbols 0 to 12
/// have 4 bits and 13 to 18 have 5 (13/16 + 6/32 = 1). By section 3.2.2, the 4-bit codes are
/// `0000` to `1100` in symbol order; the 5-bit codes start at `(1100 + 1) << 1 = 11010`.
fn length_symbol(w: &mut Writer, symbol: u32) -> &mut Writer {
    if symbol <= 12 {
        w.code(symbol, 4)
    } else {
        w.code(0b11010 + symbol - 13, 5)
    }
}

/// A dynamic block's header for literal/length lengths `literals` and distance lengths
/// `distances`, each length written as its own code-length symbol.
fn dynamic(w: &mut Writer, last: bool, literals: &[u8], distances: &[u8]) {
    w.block(last, 2);
    let (hlit, hdist) = (literals.len() - 257, distances.len() - 1);
    w.value(u32::try_from(hlit).unwrap(), 5).value(u32::try_from(hdist).unwrap(), 5).value(15, 4);
    // The code-length lengths, in the order of section 3.2.7.
    for symbol in [16, 17, 18, 0, 8, 7, 9, 6, 10, 5, 11, 4, 12, 3, 13, 2, 14, 1, 15] {
        w.value(if symbol <= 12 { 4 } else { 5 }, 3);
    }
    for &length in literals.iter().chain(distances) {
        length_symbol(w, u32::from(length));
    }
}

/// Literal/length lengths with `lengths` set and the rest zero.
fn lengths<const N: usize>(set: &[(usize, u8)]) -> [u8; N] {
    let mut all = [0; N];
    for &(symbol, length) in set {
        all[symbol] = length;
    }
    all
}

/// One fixed block, final, holding "aaaa" as a literal and a match, built by hand:
///
/// ```text
/// field        value  code (sent first to last)   bits on the wire, in order
/// BFINAL       1                                   1
/// BTYPE        01     (value, LSB first)           1 0
/// 'a' = 97     lit    00110000 + 97 = 10010001     1 0 0 1 0 0 0 1
/// length 3     257    0000000 + 1  = 0000001       0 0 0 0 0 0 1
/// distance 1   0      00000                        0 0 0 0 0
/// end          256    0000000                      0 0 0 0 0 0 0
/// ```
///
/// The 30 bits, packed least significant first: `1101_0010 0010_0000 0100_0000 0000_00`, read
/// right to left per byte: 0x4b, 0x04, 0x02, 0x00. The header is `78 01` (0x7801 = 31 * 991),
/// the trailer Adler-32("aaaa") = 0x03ce0185 (A = 1 + 4 * 97 = 389, B = 98 + 195 + 292 + 389 =
/// 974 = 0x3ce).
#[test]
fn a_hand_assembled_fixed_block_decodes() {
    let stream = [0x78, 0x01, 0x4b, 0x04, 0x02, 0x00, 0x03, 0xce, 0x01, 0x85];
    assert_eq!(inflate_zlib(&stream, ROOM).unwrap(), b"aaaa");
}

/// One dynamic block, final, holding "aa" with two one-bit literal/length codes and the single
/// one-bit distance code of section 3.2.7, built by hand:
///
/// ```text
/// BFINAL 1, BTYPE 10                    1 | 0 1
/// HLIT 0 (257), HDIST 0 (1), HCLEN 14   00000 | 00000 | 0111
/// code-length lengths, order 16 17 18 0 8 7 9 6 10 5 11 4 12 3 13 2 14 1:
///   18 and 1 have length 1, the rest 0  000 000 100 000 ... 000 100   (18 fields, 3 bits each)
/// code-length code (section 3.2.2): symbol 1 -> 0, symbol 18 -> 1
/// lengths: 97 zeros   18 + 86 (7 bits)   1 | 0110101
///          'a' = 1    1                  0
///          138 zeros  18 + 127           1 | 1111111
///          20 zeros   18 + 9             1 | 1001000
///          256 = 1    1                  0
///          dist 0 = 1 1                  0
/// literal/length code: 97 -> 0, 256 -> 1; data 'a' 'a' end   0 0 1
/// ```
///
/// 101 bits; packed, `05 c0 81 00 00 00 00 00 90 56 ff 13 10`. Adler-32("aa") = 0x012500c3 (A
/// = 195, B = 98 + 195 = 293 = 0x125). The stream was checked to decompress to "aa" with
/// CPython's zlib.
#[test]
fn a_hand_assembled_dynamic_block_with_one_distance_code_decodes() {
    let stream = [
        0x78, 0x01, 0x05, 0xc0, 0x81, 0x00, 0x00, 0x00, 0x00, 0x00, 0x90, 0x56, 0xff, 0x13, 0x10,
        0x01, 0x25, 0x00, 0xc3,
    ];
    assert_eq!(inflate_zlib(&stream, ROOM).unwrap(), b"aa");
}

/// RFC 1950's smallest stream: a final fixed block holding only its end code, `03 00`.
#[test]
fn the_empty_stream_decodes_to_nothing_even_with_a_zero_limit() {
    let stream = [0x78, 0x9c, 0x03, 0x00, 0x00, 0x00, 0x00, 0x01];
    assert_eq!(inflate_zlib(&stream, OutputLimit::new(0)).unwrap(), b"");
}

#[test]
fn a_header_that_is_not_a_multiple_of_31_is_refused() {
    refused(&[0x78, 0x02, 0x03, 0x00], InflateErrorKind::HeaderCheck { cmf: 0x78, flg: 0x02 }, 1);
}

#[test]
fn a_method_other_than_deflate_is_refused() {
    let cmf = 0x77;
    refused(&[cmf, flg_for(cmf, 0)], InflateErrorKind::CompressionMethod { method: 7 }, 1);
}

#[test]
fn a_window_field_of_7_is_accepted_and_8_is_refused() {
    let mut w = Writer::zlib(0x78, flg_for(0x78, 0));
    w.block(true, 1).code(0, 7);
    assert_eq!(inflate_zlib(&w.trailer(b""), ROOM).unwrap(), b"");
    refused(&[0x88, flg_for(0x88, 0)], InflateErrorKind::WindowSize { cinfo: 8 }, 1);
}

#[test]
fn a_preset_dictionary_is_refused_by_name() {
    refused(&[0x78, flg_for(0x78, 0x20)], InflateErrorKind::PresetDictionary, 1);
}

#[test]
fn the_reserved_block_type_is_refused() {
    let mut w = Writer::deflate();
    w.block(true, 3);
    refused(&w.finish(), InflateErrorKind::ReservedBlockType, w.last_byte());
}

#[test]
fn a_stored_length_that_does_not_match_its_complement_is_refused() {
    let mut w = Writer::deflate();
    w.block(true, 0).align().bytes(&[5, 0, 0xfa, 0xfe]);
    let kind = InflateErrorKind::StoredLengthMismatch { len: 5, nlen: 0xfefa };
    refused(&w.finish(), kind, w.last_byte());
}

#[test]
fn a_stored_block_of_the_largest_length_decodes() {
    let data: Vec<u8> = (0..65_535_u32).map(|i| u8::try_from(i % 251).unwrap()).collect();
    let mut w = Writer::deflate();
    w.block(true, 0).align().bytes(&[0xff, 0xff, 0, 0]).bytes(&data);
    assert_eq!(inflate_zlib(&w.trailer(&data), ROOM).unwrap(), data);
}

#[test]
fn literal_length_codes_are_accepted_up_to_286_and_refused_at_287() {
    let mut literals = lengths::<286>(&[(97, 1), (256, 1)]);
    let mut w = Writer::deflate();
    dynamic(&mut w, true, &literals, &[0]);
    w.code(0, 1).code(1, 1);
    assert_eq!(inflate_zlib(&w.trailer(b"a"), ROOM).unwrap(), b"a");

    let mut w = Writer::deflate();
    w.block(true, 2).value(30, 5).value(0, 5).value(15, 4);
    let kind = InflateErrorKind::TooManyCodes { alphabet: Alphabet::LiteralLength, count: 287 };
    refused(&w.finish(), kind, w.last_byte());
    literals[0] = 0;
}

#[test]
fn distance_codes_are_accepted_up_to_30_and_refused_at_31() {
    let literals = lengths::<257>(&[(97, 1), (256, 1)]);
    let mut w = Writer::deflate();
    dynamic(&mut w, true, &literals, &[0; 30]);
    w.code(0, 1).code(1, 1);
    assert_eq!(inflate_zlib(&w.trailer(b"a"), ROOM).unwrap(), b"a");

    let mut w = Writer::deflate();
    w.block(true, 2).value(0, 5).value(30, 5).value(15, 4);
    let kind = InflateErrorKind::TooManyCodes { alphabet: Alphabet::Distance, count: 31 };
    refused(&w.finish(), kind, w.last_byte());
}

/// Code-length lengths written in section 3.2.7's order, `HCLEN` + 4 of them.
fn code_length_header(lengths: &[u32]) -> Writer {
    let mut w = Writer::deflate();
    w.block(true, 2).value(0, 5).value(0, 5).value(u32::try_from(lengths.len() - 4).unwrap(), 4);
    for &length in lengths {
        w.value(length, 3);
    }
    w
}

#[test]
fn an_over_subscribed_code_length_code_is_refused() {
    // Symbols 16, 17 and 18 each with one bit: three codes where two fit.
    let w = code_length_header(&[1, 1, 1, 0]);
    let kind = InflateErrorKind::OverSubscribedCode { alphabet: Alphabet::CodeLength };
    refused(&w.finish(), kind, w.last_byte());
}

#[test]
fn a_code_length_code_of_one_symbol_is_refused_as_incomplete() {
    // Only symbol 18, with one bit: the one-code shape is admitted for no code-length code.
    let w = code_length_header(&[0, 0, 1, 0]);
    let kind = InflateErrorKind::IncompleteCode { alphabet: Alphabet::CodeLength };
    refused(&w.finish(), kind, w.last_byte());
}

#[test]
fn an_incomplete_literal_length_code_is_refused() {
    // 'a' and 256 with two bits each: half the code space unused.
    let mut w = Writer::deflate();
    dynamic(&mut w, true, &lengths::<257>(&[(97, 2), (256, 2)]), &[0]);
    let kind = InflateErrorKind::IncompleteCode { alphabet: Alphabet::LiteralLength };
    refused(&w.finish(), kind, w.last_byte());
}

#[test]
fn an_over_subscribed_distance_code_is_refused() {
    let mut w = Writer::deflate();
    dynamic(&mut w, true, &lengths::<257>(&[(97, 1), (256, 1)]), &[1, 1, 1]);
    let kind = InflateErrorKind::OverSubscribedCode { alphabet: Alphabet::Distance };
    refused(&w.finish(), kind, w.last_byte());
}

#[test]
fn an_incomplete_distance_code_of_two_symbols_is_refused() {
    let mut w = Writer::deflate();
    dynamic(&mut w, true, &lengths::<257>(&[(97, 1), (256, 1)]), &[2, 2]);
    let kind = InflateErrorKind::IncompleteCode { alphabet: Alphabet::Distance };
    refused(&w.finish(), kind, w.last_byte());
}

#[test]
fn a_literal_length_code_of_only_the_end_symbol_decodes_an_empty_block() {
    let mut w = Writer::deflate();
    dynamic(&mut w, true, &lengths::<257>(&[(256, 1)]), &[0]);
    w.code(0, 1);
    assert_eq!(inflate_zlib(&w.trailer(b""), ROOM).unwrap(), b"");
}

#[test]
fn the_unused_half_of_a_one_symbol_distance_code_is_refused() {
    // 257 -> 0, 'a' -> 10, 256 -> 11 (section 3.2.2); one distance code, 0 -> 0.
    let mut w = Writer::deflate();
    dynamic(&mut w, true, &lengths::<258>(&[(97, 2), (256, 2), (257, 1)]), &[1]);
    w.code(0b10, 2).code(0, 1).code(1, 1);
    refused(&w.finish(), InflateErrorKind::UnusedCode { alphabet: Alphabet::Distance }, w.last_byte());
}

#[test]
fn a_match_in_a_block_with_no_distance_code_is_refused() {
    let mut w = Writer::deflate();
    dynamic(&mut w, true, &lengths::<258>(&[(97, 2), (256, 2), (257, 1)]), &[0]);
    w.code(0b10, 2).code(0, 1);
    refused(&w.finish(), InflateErrorKind::UnusedCode { alphabet: Alphabet::Distance }, w.last_byte());
}

#[test]
fn a_repeat_with_no_previous_length_is_refused() {
    let mut w = Writer::deflate();
    w.block(true, 2).value(0, 5).value(0, 5).value(15, 4);
    for symbol in [16, 17, 18, 0, 8, 7, 9, 6, 10, 5, 11, 4, 12, 3, 13, 2, 14, 1, 15] {
        w.value(if symbol <= 12 { 4 } else { 5 }, 3);
    }
    length_symbol(&mut w, 16);
    refused(&w.finish(), InflateErrorKind::RepeatWithoutLength, w.last_byte());
}

#[test]
fn lengths_that_run_past_the_declared_codes_are_refused() {
    // 258 lengths declared; 138 + 138 zeros run past them.
    let mut w = Writer::deflate();
    w.block(true, 2).value(0, 5).value(0, 5).value(15, 4);
    for symbol in [16, 17, 18, 0, 8, 7, 9, 6, 10, 5, 11, 4, 12, 3, 13, 2, 14, 1, 15] {
        w.value(if symbol <= 12 { 4 } else { 5 }, 3);
    }
    length_symbol(&mut w, 18).value(127, 7);
    length_symbol(&mut w, 18).value(127, 7);
    refused(&w.finish(), InflateErrorKind::CodeLengthsOverrun, w.last_byte());
}

#[test]
fn a_block_whose_end_symbol_has_no_code_is_refused() {
    let mut w = Writer::deflate();
    dynamic(&mut w, true, &lengths::<257>(&[(97, 1), (98, 1)]), &[0]);
    refused(&w.finish(), InflateErrorKind::MissingEndOfBlock, w.last_byte());
}

#[test]
fn fixed_literal_length_symbols_286_and_287_are_refused() {
    for (symbol, code) in [(286, 0b1100_0110), (287, 0b1100_0111)] {
        let mut w = Writer::deflate();
        w.block(true, 1).code(code, 8);
        let kind = InflateErrorKind::InvalidSymbol { alphabet: Alphabet::LiteralLength, symbol };
        refused(&w.finish(), kind, w.last_byte());
    }
}

#[test]
fn fixed_distance_symbols_30_and_31_are_refused() {
    for symbol in [30, 31] {
        let mut w = Writer::deflate();
        w.block(true, 1).fixed_literal(b'a').code(1, 7).code(u32::from(symbol), 5);
        let kind = InflateErrorKind::InvalidSymbol { alphabet: Alphabet::Distance, symbol };
        refused(&w.finish(), kind, w.last_byte());
    }
}

#[test]
fn a_distance_to_the_first_byte_decodes_and_one_further_is_refused() {
    // 'a' 'b', then length 3 at distance 2 (symbol 1, no extra bits): "ababa".
    let mut w = Writer::deflate();
    w.block(true, 1).fixed_literal(b'a').fixed_literal(b'b').code(1, 7).code(1, 5).code(0, 7);
    assert_eq!(inflate_zlib(&w.trailer(b"ababa"), ROOM).unwrap(), b"ababa");

    // Distance 3 (symbol 2) with two bytes decoded.
    let mut w = Writer::deflate();
    w.block(true, 1).fixed_literal(b'a').fixed_literal(b'b').code(1, 7).code(2, 5);
    let kind = InflateErrorKind::DistanceTooFar { distance: 3, available: 2 };
    refused(&w.finish(), kind, w.last_byte());
}

/// A header with `CINFO` 0 (a 256-byte window), 300 literals, then a 3-byte match at
/// `distance` (256 is symbol 15, base 193, 6 extra bits of 63; 257 is symbol 16, base 257, 7
/// extra bits of 0).
fn small_window(distance: u32) -> (Writer, Vec<u8>) {
    let mut w = Writer::zlib(0x08, flg_for(0x08, 0));
    let literals: Vec<u8> = (0..300_u32).map(|i| u8::try_from(i % 256).unwrap()).collect();
    w.block(true, 1);
    for &b in &literals {
        w.fixed_literal(b);
    }
    w.code(1, 7);
    if distance == 256 {
        w.code(15, 5).value(63, 6);
    } else {
        w.code(16, 5).value(distance - 257, 7);
    }
    (w, literals)
}

#[test]
fn a_distance_equal_to_the_declared_window_decodes_and_one_past_is_refused() {
    let (mut w, mut decoded) = small_window(256);
    w.code(0, 7);
    let start = decoded.len() - 256;
    decoded.extend_from_within(start..start + 3);
    assert_eq!(inflate_zlib(&w.trailer(&decoded), ROOM).unwrap(), decoded);

    let (w, _) = small_window(257);
    let kind = InflateErrorKind::DistanceBeyondWindow { distance: 257, window: 256 };
    refused(&w.finish(), kind, w.last_byte());
}

#[test]
fn a_stream_cut_inside_its_header_is_truncated_at_its_length() {
    refused(&[0x78], InflateErrorKind::Truncated, 1);
    refused(&[], InflateErrorKind::Truncated, 0);
}

#[test]
fn a_wrong_adler32_is_refused() {
    let stream = [0x78, 0x9c, 0x03, 0x00, 0x00, 0x00, 0x00, 0x02];
    let kind = InflateErrorKind::Adler32Mismatch { stored: 2, computed: adler32(b"") };
    refused(&stream, kind, 7);
}

#[test]
fn a_byte_after_the_trailer_is_refused() {
    let stream = [0x78, 0x9c, 0x03, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00];
    refused(&stream, InflateErrorKind::TrailingBytes { count: 1 }, 8);
}

#[test]
fn a_stored_block_beyond_the_limit_is_refused_before_its_data_is_read() {
    // The block declares 10 bytes and holds none: the limit, not the truncation, is reported.
    let mut w = Writer::deflate();
    w.block(true, 0).align().bytes(&[10, 0, 0xf5, 0xff]);
    let error = inflate_zlib(&w.finish(), OutputLimit::new(9)).unwrap_err();
    assert_eq!((error.kind(), error.offset()), (&InflateErrorKind::OutputLimit { limit: 9 }, 6));
}

#[test]
fn a_match_beyond_the_limit_is_refused_at_the_match() {
    // "aaaa" is 'a' and a 3-byte match: a limit of 3 admits the literal and refuses the match.
    let stream = [0x78, 0x01, 0x4b, 0x04, 0x02, 0x00, 0x03, 0xce, 0x01, 0x85];
    let error = inflate_zlib(&stream, OutputLimit::new(3)).unwrap_err();
    assert_eq!(error.kind(), &InflateErrorKind::OutputLimit { limit: 3 });
    assert_eq!(inflate_zlib(&stream, OutputLimit::new(4)).unwrap(), b"aaaa");
}
