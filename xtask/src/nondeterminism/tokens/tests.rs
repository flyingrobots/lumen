//! The tokenizer on the Rust reference's literal forms, shebang lines and raw identifiers.
//!
//! Oracle: the Rust reference's grammar (`tokens.html`: integer, tuple-index and float literals,
//! and the shebang rule of `input-format.html`), with each expected token written by hand; the
//! forms the reference states compile were checked with `rustc` 1.96.0.

use super::{is_float, tokens, Token};

/// The number tokens of `source`, each with whether it is a float.
fn numbers(source: &str) -> Vec<(String, bool)> {
    tokens(source)
        .into_iter()
        .filter_map(
            |(_, t)| if let Token::Num(n) = t { Some((n.clone(), is_float(&n))) } else { None },
        )
        .collect()
}

fn num(text: &str, float: bool) -> (String, bool) {
    (text.to_owned(), float)
}

#[test]
fn the_references_float_literals_are_floats() {
    for float in [
        "123.0f64",
        "0.1f64",
        "0.1f32",
        "12E+99_f64",
        "2.",
        "1e3",
        "1E-3",
        "2.5e+7",
        "5f32",
        "2_f64",
        "1_000.0",
        "1e+_3",
        "1e_3",
        "1_e+3",
        "1.0_f64",
        "0e0",
    ] {
        assert_eq!(numbers(&format!("let x = {float};")), [num(float, true)], "{float}");
    }
}

#[test]
fn the_references_integer_literals_are_not_floats() {
    for integer in [
        "123",
        "123i32",
        "123_u32",
        "0xff",
        "0xff_u8",
        "0x01_f32",
        "0x01_e3",
        "0o70_i16",
        "0b1111_1111_1001_0000i64",
        "0b________1",
        "0usize",
        "1usize",
        "0xf64",
    ] {
        assert_eq!(numbers(&format!("let x = {integer};")), [num(integer, false)], "{integer}");
    }
}

#[test]
fn a_range_bound_after_two_dots_is_a_float_but_a_tuple_index_is_not() {
    assert_eq!(numbers("let r = ..1.5;"), [num("1.5", true)]);
    assert_eq!(numbers("let r = 0..1.5;"), [num("0", false), num("1.5", true)]);
    assert_eq!(numbers("let r = ..=2.5;"), [num("2.5", true)]);
    assert_eq!(numbers("let t = p.0.1;"), [num("0", false), num("1", false)]);
    assert_eq!(numbers("let t = p.0 .. 1;"), [num("0", false), num("1", false)]);
}

#[test]
fn look_alikes_of_float_literals_are_integers() {
    let int = |t: &str| num(t, false);
    assert_eq!(numbers("let r = 1..2;"), [int("1"), int("2")]);
    assert_eq!(numbers("let m = 1.foo();"), [int("1")]);
    assert_eq!(numbers("let f = 2.f64;"), [int("2")]);
    assert_eq!(numbers("let h = 0x1e+3;"), [int("0x1e"), int("3")]);
    assert_eq!(numbers("let s = 1usize+2;"), [int("1usize"), int("2")]);
    assert_eq!(numbers("let s = 1usize-2;"), [int("1usize"), int("2")]);
    assert_eq!(numbers("let s = e+1;"), [int("1")]);
    assert_eq!(numbers("let s = 1e3+2;"), [num("1e3", true), int("2")]);
}

#[test]
fn a_character_literal_quote_does_not_open_a_string() {
    assert_eq!(numbers("let q = ('\"', 1.5, \"\");"), [num("1.5", true)]);
}

#[test]
fn a_shebang_line_is_skipped_and_the_rest_is_read() {
    let hidden = tokens("#!/bin/x /*\nconst A: u32 = 1;\n");
    assert_eq!(hidden.first(), Some(&(2, Token::Ident("const".to_owned()))));
    let bom = tokens("\u{feff}#!/bin/x /*\nconst A: u32 = 1;\n");
    assert_eq!(bom.first(), Some(&(2, Token::Ident("const".to_owned()))));
    for doc in ["#! /// doc\n[x]\n", "#! //! doc\n[x]\n", "#! /** doc */\n[x]\n", "#!/*! d */\n[x]"]
    {
        assert_eq!(tokens(doc).first(), Some(&(2, Token::Punct('['))), "{doc:?}");
    }
}

#[test]
fn an_inner_attribute_after_hash_bang_is_not_a_shebang() {
    for source in ["#![allow(x)]", "#! [x]", "#!\n[x]", "#! /* c /* d */ */ [x]", "#! // c\n[x]"] {
        assert_eq!(tokens(source).first(), Some(&(1, Token::Punct('#'))), "{source:?}");
    }
    assert_eq!(tokens("\n#!/x").first(), Some(&(2, Token::Punct('#'))));
}

#[test]
fn a_raw_identifier_is_read_without_its_prefix() {
    let idents = |source: &str| {
        tokens(source)
            .into_iter()
            .filter_map(|(_, t)| if let Token::Ident(i) = t { Some(i) } else { None })
            .collect::<Vec<_>>()
    };
    assert_eq!(idents("std::r#fs::copy"), ["std", "fs", "copy"]);
    assert_eq!(idents("let r#type = r#f64;"), ["let", "type", "f64"]);
    assert_eq!(tokens("r#\"a\"#").first().map(|(_, t)| t), Some(&Token::Str("a".to_owned())));
}
