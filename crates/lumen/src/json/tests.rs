//! Unit tests of the reader: limits exactly on and one past, every truncation, positions.
//!
//! Oracles: the limits and positions are written by hand from the contract in the module
//! documentation, never computed by the reader. Truncation uses documents whose validity is
//! known by construction: a proper prefix of a document whose top level is a container or a
//! string is never valid JSON, since the closing bracket or quote is the last byte.

use super::*;

fn parse(text: &[u8]) -> Result<JsonValue, JsonError> {
    parse_json(text, JsonLimits::default())
}

fn nested(depth: usize) -> Vec<u8> {
    let mut doc = vec![b'['; depth];
    doc.extend(vec![b']'; depth]);
    doc
}

#[test]
fn the_reader_has_an_identity() {
    assert_eq!(JSON_READER_ID, "lumen-json-reader/1");
}

#[test]
fn nesting_exactly_at_the_depth_limit_is_read() {
    let limits = JsonLimits::new(5, 1000).unwrap();
    assert!(parse_json(&nested(5), limits).is_ok());
}

#[test]
fn nesting_one_past_the_depth_limit_is_refused_at_the_bracket() {
    let limits = JsonLimits::new(5, 1000).unwrap();
    let error = parse_json(&nested(6), limits).unwrap_err();
    assert_eq!(error.kind(), &JsonErrorKind::TooDeep { limit: 5 });
    assert_eq!((error.offset(), error.line(), error.column()), (5, 1, 6));
}

#[test]
fn objects_count_toward_depth_like_arrays() {
    let limits = JsonLimits::new(2, 1000).unwrap();
    assert!(parse_json(br#"{"a":{}}"#, limits).is_ok());
    let error = parse_json(br#"{"a":{"b":{}}}"#, limits).unwrap_err();
    assert_eq!(error.kind(), &JsonErrorKind::TooDeep { limit: 2 });
    assert_eq!(error.offset(), 11);
}

#[test]
fn depth_zero_admits_scalars_only() {
    let limits = JsonLimits::new(0, 1000).unwrap();
    assert!(parse_json(b"1", limits).is_ok());
    assert_eq!(parse_json(b"[]", limits).unwrap_err().offset(), 0);
}

#[test]
fn too_deep_is_refused_before_the_rest_is_read() {
    // Unterminated and past the limit: the depth error comes first, so nothing deeper was stored.
    let limits = JsonLimits::new(3, 1000).unwrap();
    let error = parse_json(b"[[[[[[[[", limits).unwrap_err();
    assert_eq!(error.kind(), &JsonErrorKind::TooDeep { limit: 3 });
}

#[test]
fn the_depth_ceiling_is_accepted_and_one_past_it_is_refused() {
    assert!(JsonLimits::new(JSON_MAX_DEPTH_CEILING, 1).is_ok());
    let error = JsonLimits::new(JSON_MAX_DEPTH_CEILING + 1, 1).unwrap_err();
    assert_eq!(error.max_depth(), 1025);
}

#[test]
fn a_document_at_the_ceiling_is_read_and_dropped_on_a_test_thread() {
    let limits = JsonLimits::new(JSON_MAX_DEPTH_CEILING, 1 << 20).unwrap();
    let value = parse_json(&nested(JSON_MAX_DEPTH_CEILING), limits).unwrap();
    assert_eq!(value.clone(), value);
}

#[test]
fn input_exactly_at_the_size_limit_is_read() {
    let limits = JsonLimits::new(4, 7).unwrap();
    assert!(parse_json(b"[1,2,3]", limits).is_ok());
}

#[test]
fn input_one_byte_past_the_size_limit_is_refused_before_being_read() {
    let limits = JsonLimits::new(4, 7).unwrap();
    // Invalid after byte 7 as well, but the size is what is reported, at the first byte past it.
    let error = parse_json(b"[1,2,3]]", limits).unwrap_err();
    assert_eq!(error.kind(), &JsonErrorKind::TooLarge { length: 8, limit: 7 });
    assert_eq!((error.offset(), error.line(), error.column()), (7, 1, 8));
}

#[test]
fn a_zero_byte_limit_still_reports_empty_input_as_an_end() {
    let limits = JsonLimits::new(4, 0).unwrap();
    let error = parse_json(b"", limits).unwrap_err();
    assert!(matches!(error.kind(), JsonErrorKind::UnexpectedEnd { .. }));
    assert!(matches!(
        parse_json(b"1", limits).unwrap_err().kind(),
        JsonErrorKind::TooLarge { .. }
    ));
}

#[test]
fn the_defaults_are_the_stated_values() {
    let d = JsonLimits::default();
    assert_eq!((d.max_depth(), d.max_bytes()), (128, 16_777_216));
}

const DOCUMENTS: &[&str] = &[
    r#"{"a":[1,-2.5e+3,true,false,null,"xé😀\n"],"b":{"c":{}},"d":[]}"#,
    "[\n  {\"k\": \"v\"},\n  [[], [[]]],\n  0\n]",
    r#""a string with \"escapes\" and \\ and é and 😀""#,
    r#"{"é😀":"\/","":[0,-0,0.0,1E400]}"#,
];

#[test]
fn every_proper_prefix_of_a_container_or_string_document_is_refused_with_a_position() {
    for doc in DOCUMENTS {
        let bytes = doc.as_bytes();
        assert!(parse(bytes).is_ok(), "{doc}");
        for cut in 0..bytes.len() {
            let error = parse(&bytes[..cut]).expect_err(&format!("{doc} cut at {cut}"));
            assert!(error.line() >= 1 && error.column() >= 1);
            assert!(error.offset() <= cut, "{doc} cut at {cut}: {error}");
        }
    }
}

#[test]
fn every_truncation_of_a_scalar_document_is_refused_or_is_itself_valid() {
    // Oracle: the valid prefixes of `-12.5e+3` are those that end in a digit and are complete
    // numbers: `-1`, `-12`, `-12.5` and `-12.5e+3`; the others must be refused.
    let doc = b"-12.5e+3";
    let valid = [2usize, 3, 5, 8];
    for cut in 1..=doc.len() {
        assert_eq!(parse(&doc[..cut]).is_ok(), valid.contains(&cut), "cut at {cut}");
    }
}

#[test]
fn positions_count_characters_in_columns_and_line_feeds_in_lines() {
    // Line 2 is `é😀 x`: the stray `x` follows 3 characters (5 bytes... é is 2, 😀 is 4).
    let error = parse("[\"\",\n\"é😀\" x]".as_bytes()).unwrap_err();
    assert_eq!((error.line(), error.column()), (2, 5));
    assert_eq!(error.offset(), 5 + 1 + 2 + 4 + 1 + 1 - 1 + 0);
}

#[test]
fn a_carriage_return_is_a_character_of_its_line() {
    let error = parse(b"[1,\r\n2 3]").unwrap_err();
    assert_eq!((error.line(), error.column()), (2, 3));
    let error = parse(b"[1,\r2 3]").unwrap_err();
    assert_eq!((error.line(), error.column()), (1, 6));
}

#[test]
fn an_error_at_the_end_is_at_the_length() {
    let error = parse(b"[1,\n").unwrap_err();
    assert_eq!((error.offset(), error.line(), error.column()), (4, 2, 1));
}

#[test]
fn display_names_line_and_column() {
    let text = parse(b"[1,]").unwrap_err().to_string();
    assert!(text.starts_with("line 1, column 4: "), "{text}");
}

#[test]
fn accessors_read_the_tree() {
    let v = parse(br#"{"a":[1],"s":"t","n":2.50}"#).unwrap();
    assert_eq!(v.get("a").and_then(JsonValue::as_array).map(<[_]>::len), Some(1));
    assert_eq!(v.get("s").and_then(JsonValue::as_str), Some("t"));
    assert_eq!(v.get("n").and_then(JsonValue::as_number), Some("2.50"));
    assert_eq!(v.as_object().map(<[_]>::len), Some(3));
    assert_eq!(v.get("zz"), None);
    assert_eq!(JsonValue::Null.get("a"), None);
}

#[test]
fn members_keep_document_order() {
    let v = parse(br#"{"b":1,"a":2,"c":3}"#).unwrap();
    let keys: Vec<&str> = v.as_object().unwrap().iter().map(|(k, _)| k.as_str()).collect();
    assert_eq!(keys, ["b", "a", "c"]);
}
