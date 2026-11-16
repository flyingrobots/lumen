//! The JSON writer: each kind of value and every class of character, against text written by
//! hand from RFC 8259.

use super::*;

fn string(text: &str) -> String {
    let mut out = String::new();
    write_string(&mut out, text);
    out
}

/// Oracle: RFC 8259, section 7, by hand. The two characters that must be escaped, the five short
/// escapes, and the control range exactly at its ends: U+0000 and U+001F are escaped, U+0020
/// (the first character past the range) and U+007F (DEL, which JSON does not escape) are not.
#[test]
fn strings_escape_quotes_backslashes_and_exactly_the_control_characters() {
    assert_eq!(string(""), r#""""#);
    assert_eq!(string("a\"b"), r#""a\"b""#);
    assert_eq!(string("a\\b"), r#""a\\b""#);
    assert_eq!(string("/"), r#""/""#);
    assert_eq!(string("\u{8}\t\n\u{c}\r"), r#""\b\t\n\f\r""#);
    assert_eq!(string("\u{0}"), r#""\u0000""#);
    assert_eq!(string("\u{1}"), r#""\u0001""#);
    assert_eq!(string("\u{b}"), r#""\u000b""#);
    assert_eq!(string("\u{1f}"), r#""\u001f""#);
    assert_eq!(string("\u{20}"), r#"" ""#);
    assert_eq!(string("\u{7f}"), "\"\u{7f}\"");
}

/// Oracle: every character from U+0000 to U+00FF, by its class: below U+0020 the output is an
/// escape (six bytes, or two for the short forms), `"` and `\` are two bytes, and every other
/// character is itself. Witness count: 256 characters.
#[test]
fn every_character_up_to_u_00ff_is_escaped_only_if_it_must_be() {
    let short = [('\u{8}', 'b'), ('\t', 't'), ('\n', 'n'), ('\u{c}', 'f'), ('\r', 'r')];
    for code in 0..=0xff_u32 {
        let c = char::from_u32(code).expect("a scalar value");
        let inner = string(&c.to_string());
        let inner = &inner[1..inner.len() - 1];
        let letter = short.iter().find(|(s, _)| *s == c).map(|(_, letter)| *letter);
        let expected = match (c, letter) {
            ('"' | '\\', _) => format!("\\{c}"),
            (_, Some(letter)) => format!("\\{letter}"),
            _ if code < 0x20 => format!("\\u00{code:02x}"),
            _ => c.to_string(),
        };
        assert_eq!(inner, expected, "U+{code:04X}");
        if code < 0x20 {
            assert!(inner.starts_with('\\') && inner.is_ascii(), "U+{code:04X}: {inner}");
        }
    }
}

/// Oracle: the UTF-8 bytes of each character, written as itself.
#[test]
fn non_ascii_is_written_as_utf_8() {
    assert_eq!(string("é"), "\"\u{e9}\"");
    assert_eq!(string("日本"), "\"\u{65e5}\u{672c}\"");
    assert_eq!(string("\u{1f600}").as_bytes(), b"\"\xf0\x9f\x98\x80\"");
    assert_eq!(string("\u{10ffff}").as_bytes(), b"\"\xf4\x8f\xbf\xbf\"");
}

/// Oracle: decimal by hand, at both ends of `u64`.
#[test]
fn integers_are_decimal_at_both_ends_of_their_range() {
    let line = |value: Value| Object::new().with("n", value).to_json();
    assert_eq!(line(Value::from(0_u64)), r#"{"n":0}"#);
    assert_eq!(line(Value::from(u64::MAX)), r#"{"n":18446744073709551615}"#);
    assert_eq!(line(Value::from(u32::MAX)), r#"{"n":4294967295}"#);
    assert_eq!(line(Value::from(usize::MAX)), format!(r#"{{"n":{}}}"#, usize::MAX));
}

/// Oracle: the text by hand. Members keep their order; `None` is `null`; nesting closes in order.
#[test]
fn objects_and_arrays_keep_their_order_and_nest() {
    let inner = Object::new().with("z", true).with("a", false);
    let object = Object::new()
        .with("b", Value::Null)
        .with("a", inner)
        .with("list", array([1_u32, 2, 3]))
        .with("empty", array(Vec::<u32>::new()))
        .with("none", None::<u32>)
        .with("some", Some("x"))
        .with("o", Object::new());
    assert_eq!(
        object.to_json(),
        r#"{"b":null,"a":{"z":true,"a":false},"list":[1,2,3],"empty":[],"none":null,"some":"x","o":{}}"#
    );
}

/// Oracle: a key is a string like any other, so a key that needed escaping would be escaped.
#[test]
fn keys_are_written_as_strings() {
    assert_eq!(Object::new().with("a\"b", 1_u32).to_json(), r#"{"a\"b":1}"#);
}
