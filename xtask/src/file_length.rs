//! The file-length gate: the Rust standard's limits on lines of code per file.
//!
//! A source file may hold 300 lines of code; from 301 to 500 it needs a listed exception that
//! names its decomposition issue, and above 500 it does not merge at all. A test file (one the
//! compiler builds only into tests, [`Role::Test`]) may hold 750, with no exception past that.
//! A line of code is one with a character
//! outside a comment ([`code_lines`]). An exception records the count it was granted at; the
//! file may shrink but not grow past it, and an exception that is no longer needed is itself a
//! failure, so the list only gets shorter.

pub(crate) mod count;
mod exceptions;
#[cfg(test)]
mod tests;

pub(crate) use count::code_lines;
pub(crate) use exceptions::EXCEPTIONS;

use crate::layout::Role;

/// Lines of code a source file may hold without an exception.
pub(crate) const SOURCE_REVIEW: usize = 300;
/// Lines of code above which a source file does not merge without an exception.
pub(crate) const SOURCE_MAX: usize = 500;
/// Lines of code a test file may hold without an exception.
pub(crate) const TEST_MAX: usize = 750;

/// A file over its limit, allowed until its issue is done.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Exception {
    /// The file, relative to the repository root, with `/` separators.
    pub(crate) path: &'static str,
    /// Its lines of code when the exception was granted: the most it may hold.
    pub(crate) allowed: usize,
    /// The day the exception was granted, `YYYY-MM-DD`.
    pub(crate) since: &'static str,
    /// The issue that splits the file, `owner/repo#N`.
    pub(crate) issue: &'static str,
}

/// A file, its role and its lines of code.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Measured {
    /// The file, relative to the repository root, with `/` separators.
    pub(crate) path: String,
    /// Whether the compiler builds it only into tests ([`crate::layout::Layout::role`]).
    pub(crate) role: Role,
    /// Its lines of code ([`code_lines`]).
    pub(crate) lines: usize,
}

/// What the gate found: failures, and notices for the files held by an exception.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Verdict {
    /// Each a reason the gate fails.
    pub(crate) failures: Vec<String>,
    /// Each a file over its limit under an exception.
    pub(crate) notices: Vec<String>,
}

/// The most lines of code a file of `role` may hold without an exception.
pub(crate) fn limit(role: Role) -> usize {
    match role {
        Role::Test => TEST_MAX,
        Role::Source => SOURCE_REVIEW,
    }
}

/// The most lines of code a file of `role` may hold at all: no exception goes past it.
pub(crate) fn hard_limit(role: Role) -> usize {
    match role {
        Role::Test => TEST_MAX,
        Role::Source => SOURCE_MAX,
    }
}

/// Judges every measured file against its limit and the exceptions.
pub(crate) fn judge(files: &[Measured], exceptions: &[Exception]) -> Verdict {
    let mut verdict = Verdict::default();
    for file in files {
        let (limit, hard) = (limit(file.role), hard_limit(file.role));
        match exceptions.iter().find(|e| e.path == file.path) {
            _ if file.lines <= limit => {}
            _ if file.lines > hard => verdict.failures.push(format!(
                "{}: {} lines of code, over the maximum of {hard}; no exception allows that",
                file.path, file.lines
            )),
            None => verdict.failures.push(format!(
                "{}: {} lines of code, over {limit}: split it, or list an exception that names \
                 its decomposition issue",
                file.path, file.lines
            )),
            Some(e) if file.lines > e.allowed => verdict.failures.push(format!(
                "{}: {} lines of code, more than the {} its exception ({}, since {}) allows",
                file.path, file.lines, e.allowed, e.issue, e.since
            )),
            Some(e) => verdict.notices.push(format!(
                "{}: {} lines of code, over {limit} under an exception until {} (since {})",
                file.path, file.lines, e.issue, e.since
            )),
        }
    }
    for e in exceptions {
        match files.iter().find(|f| f.path == e.path) {
            None => verdict.failures.push(format!("{}: an exception for a missing file", e.path)),
            Some(f) if f.lines <= limit(f.role) => verdict.failures.push(format!(
                "{}: {} lines of code is within the limit; remove its exception ({})",
                f.path, f.lines, e.issue
            )),
            Some(_) => {}
        }
    }
    verdict
}
