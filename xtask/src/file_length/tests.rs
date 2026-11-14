//! The gate on constructed files: exact line counts, each limit at and one past its boundary,
//! the exception rules, and the real exception list against the policy.

use super::{
    code_lines, judge, Exception, Measured, Verdict, EXCEPTIONS, SOURCE_MAX, SOURCE_REVIEW,
    TEST_MAX,
};
use crate::layout::Role;

const SRC: &str = "crates/lumen/src/big.rs";
const TEST: &str = "crates/lumen/tests/big.rs";
const UNIT: &str = "crates/lumen/src/big/tests.rs";

/// A measured file; its role is what `crate::layout` decides (tested there), here by the path.
fn file(path: &str, lines: usize) -> Measured {
    let role = if path == SRC { Role::Source } else { Role::Test };
    Measured { path: path.to_owned(), role, lines }
}

fn exception(path: &'static str, allowed: usize) -> Exception {
    Exception { path, allowed, since: "2026-11-14", issue: "flyingrobots/lumen#1" }
}

fn failures(files: &[Measured], exceptions: &[Exception]) -> usize {
    judge(files, exceptions).failures.len()
}

#[test]
fn comments_and_blank_lines_do_not_count() {
    let text = "//! doc\n\n/// doc\nfn a() {} // trailing\n   \n// x\n/* one\n two */\nfn b() {}\n";
    assert_eq!(code_lines(text), 2);
}

#[test]
fn block_comments_nest_and_code_after_one_counts() {
    assert_eq!(code_lines("/* a /* b */ still\n comment */\n"), 0);
    assert_eq!(code_lines("/* a */ fn x() {}\n"), 1);
    assert_eq!(code_lines("fn x() {} /* a\n b */ fn y() {}\n"), 2);
}

#[test]
fn comment_markers_inside_strings_and_chars_are_code() {
    assert_eq!(code_lines("let s = \"// not a comment\";\nlet t = \"/*\";\nlet u = 1;\n"), 3);
    assert_eq!(code_lines("let s = r#\"a \" /* b\"#;\nlet t = 2;\n"), 2);
    assert_eq!(code_lines("let q = '\"';\nlet e = '\\'';\nlet x = 3;\n"), 3);
    assert_eq!(code_lines("let s = \"\\\"/*\";\nlet y = 4;\n"), 2);
}

#[test]
fn a_lifetime_is_not_a_character_literal() {
    assert_eq!(code_lines("fn f<'a>(x: &'a str) -> &'a str { x }\n// c\n"), 1);
}

#[test]
fn a_multi_line_string_counts_its_lines_with_text() {
    assert_eq!(code_lines("let s = \"one\n\ntwo\";\n"), 2);
    assert_eq!(code_lines("let s = \"one \\\n  two\";\n"), 2);
}

#[test]
fn a_character_literal_quote_does_not_open_a_string() {
    assert_eq!(code_lines("let q = '\"';\n// a comment \"\n"), 1);
}

#[test]
fn a_shebang_line_is_skipped_as_rustc_skips_it() {
    let body = "fn a() {}\n".repeat(603);
    assert_eq!(code_lines(&format!("#!/bin/x /*\n{body}")), 603);
    assert_eq!(code_lines(&format!("\u{feff}#!/usr/bin/env run\n{body}")), 603);
    assert_eq!(failures(&[file(SRC, code_lines(&format!("#!/bin/x /*\n{body}")))], &[]), 1);
    assert_eq!(code_lines("#![allow(dead_code)]\nfn a() {}\n"), 2);
}

#[test]
fn a_last_line_without_a_newline_counts() {
    assert_eq!(code_lines("fn a() {}\nfn b() {}"), 2);
    assert_eq!(code_lines(""), 0);
}

#[test]
fn a_source_file_may_hold_300_lines_without_an_exception() {
    assert_eq!(judge(&[file(SRC, SOURCE_REVIEW)], &[]), Verdict::default());
    assert_eq!(failures(&[file(SRC, SOURCE_REVIEW + 1)], &[]), 1);
}

#[test]
fn an_exception_holds_a_source_file_up_to_500_and_its_granted_count() {
    let held = judge(&[file(SRC, SOURCE_MAX)], &[exception(SRC, SOURCE_MAX)]);
    assert!(held.failures.is_empty(), "{held:?}");
    assert_eq!(held.notices.len(), 1);
    assert_eq!(failures(&[file(SRC, 401)], &[exception(SRC, 400)]), 1);
}

#[test]
fn nothing_holds_a_source_file_past_500() {
    assert_eq!(failures(&[file(SRC, SOURCE_MAX + 1)], &[]), 1);
    assert_eq!(failures(&[file(SRC, SOURCE_MAX + 1)], &[exception(SRC, 600)]), 1);
}

#[test]
fn a_test_file_may_hold_750_and_no_more() {
    for path in [TEST, UNIT] {
        assert_eq!(judge(&[file(path, TEST_MAX)], &[]), Verdict::default(), "{path}");
        assert_eq!(failures(&[file(path, TEST_MAX + 1)], &[]), 1, "{path}");
        assert_eq!(failures(&[file(path, TEST_MAX + 1)], &[exception(path, 800)]), 1, "{path}");
    }
}

#[test]
fn an_exception_that_is_not_needed_fails() {
    assert_eq!(failures(&[file(SRC, SOURCE_REVIEW)], &[exception(SRC, 320)]), 1);
    assert_eq!(failures(&[], &[exception(SRC, 320)]), 1);
}

#[test]
fn every_listed_exception_is_dated_names_an_issue_and_is_in_the_review_band() {
    for e in EXCEPTIONS {
        assert!(e.allowed > SOURCE_REVIEW && e.allowed <= SOURCE_MAX, "{e:?}");
        assert!(e.issue.starts_with("flyingrobots/lumen#"), "{e:?}");
        let date: Vec<&str> = e.since.split('-').collect();
        assert!(date.len() == 3 && date.iter().all(|p| p.parse::<u32>().is_ok()), "{e:?}");
    }
}
