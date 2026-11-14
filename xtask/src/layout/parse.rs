//! The module items of one file, as tokens: its `mod` declarations with their attributes, its
//! `#![cfg(test)]`, and the forms that load a file the gates do not read.

use super::{Decl, InlineDir, Parsed};
use crate::nondeterminism::tokens::{tokens, Located, Token};

/// The `mod` declarations of `text`, whether it is `#![cfg(test)]`, and its `include!`s and
/// `cfg_attr` paths.
pub(super) fn parse(text: &str) -> Parsed {
    let toks = tokens(text);
    let mut parser = Parser {
        toks: &toks,
        parsed: Parsed { test: false, decls: Vec::new(), failures: Vec::new() },
        depth: 0,
        inline: Vec::new(),
        attrs: Vec::new(),
    };
    let mut i = 0;
    while i < toks.len() {
        i = parser.step(i);
    }
    parser.parsed
}

/// The parser's state: the brace depth, the enclosing inline modules (their directory, the depth
/// their body opens at, and whether they are under `cfg(test)`), and the outer attributes read
/// for the next item.
struct Parser<'t> {
    toks: &'t [Located],
    parsed: Parsed,
    depth: usize,
    inline: Vec<(InlineDir, usize, bool)>,
    attrs: Vec<&'t [Located]>,
}

impl Parser<'_> {
    /// Reads the token at `i`; the index of the next token to read.
    fn step(&mut self, i: usize) -> usize {
        let toks = self.toks;
        let next = |k: usize| toks.get(i + k).map(|(_, t)| t);
        let inner = next(1) == Some(&Token::Punct('!'));
        let open = i + 1 + usize::from(inner);
        let (line, token) = &toks[i];
        if *token == Token::Punct('#') && toks.get(open).map(|(_, t)| t) == Some(&Token::Punct('['))
        {
            return self.attribute(*line, inner, open);
        }
        let pending = core::mem::take(&mut self.attrs);
        match token {
            Token::Ident(word) if word == "include" && next(1) == Some(&Token::Punct('!')) => {
                self.parsed.failures.push((
                    *line,
                    "include! reads a file the gates do not read as a module: make it a module"
                        .to_owned(),
                ));
            }
            Token::Ident(word) if word == "pub" => {
                self.attrs = pending;
                if next(1) == Some(&Token::Punct('(')) {
                    return closing(toks, i + 1) + 1;
                }
            }
            Token::Ident(word) if word == "mod" => self.module(i, &pending),
            Token::Punct('{') => self.depth += 1,
            Token::Punct('}') => {
                if self.inline.last().is_some_and(|(_, d, _)| *d == self.depth) {
                    self.inline.pop();
                }
                self.depth = self.depth.saturating_sub(1);
            }
            _ => {}
        }
        i + 1
    }

    /// Reads the attribute whose `[` is at `open`; the index after its `]`.
    fn attribute(&mut self, line: usize, inner: bool, open: usize) -> usize {
        let end = closing(self.toks, open);
        let attr = &self.toks[(open + 1).min(end)..end];
        check_attribute(line, attr, &mut self.parsed.failures);
        if inner && is_cfg_test(attr) {
            // `#![cfg(test)]` at the top makes the file a test file; inside an inline module's
            // braces, that module.
            match self.inline.last_mut() {
                Some((_, open, test)) if *open == self.depth => *test = true,
                _ if self.depth == 0 => self.parsed.test = true,
                _ => {}
            }
        } else if !inner {
            self.attrs.push(attr);
        }
        end + 1
    }

    /// Reads the `mod` item at `i`, with the outer attributes `pending` read before it.
    fn module(&mut self, i: usize, pending: &[&[Located]]) {
        let next = |k: usize| self.toks.get(i + k).map(|(_, t)| t);
        let Some(Token::Ident(name)) = next(1) else { return };
        let under_test =
            pending.iter().any(|a| is_cfg_test(a)) || self.inline.iter().any(|(_, _, t)| *t);
        let file = pending.iter().find_map(|a| path_value(a));
        if next(2) == Some(&Token::Punct('{')) {
            let dir = file.map_or_else(|| InlineDir::Name(name.clone()), InlineDir::Path);
            self.inline.push((dir, self.depth + 1, under_test));
        } else if next(2) == Some(&Token::Punct(';')) {
            let dirs = self.inline.iter().map(|(d, _, _)| d.clone()).collect();
            let (line, name) = (self.toks[i].0, name.clone());
            self.parsed.decls.push(Decl { line, name, test: under_test, path: file, inline: dirs });
        }
    }
}

/// The index of the bracket that closes the one at `open` (`[`, `(` or `{`), or the last index.
fn closing(toks: &[Located], open: usize) -> usize {
    let mut depth = 0_usize;
    for (k, (_, t)) in toks.iter().enumerate().skip(open) {
        match t {
            Token::Punct('[' | '(' | '{') => depth += 1,
            Token::Punct(']' | ')' | '}') => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    return k;
                }
            }
            _ => {}
        }
    }
    toks.len().saturating_sub(1)
}

/// Whether an attribute's tokens are exactly `cfg(test)`.
fn is_cfg_test(attr: &[Located]) -> bool {
    let words: Vec<&Token> = attr.iter().map(|(_, t)| t).collect();
    matches!(words.as_slice(), [Token::Ident(c), Token::Punct('('), Token::Ident(t), Token::Punct(')')] if c == "cfg" && t == "test")
}

/// The file of a `path = "..."` attribute.
fn path_value(attr: &[Located]) -> Option<String> {
    match attr {
        [(_, Token::Ident(p)), (_, Token::Punct('=')), (_, Token::Str(file))] if p == "path" => {
            Some(file.clone())
        }
        _ => None,
    }
}

/// Adds a failure for a `path` key inside `cfg_attr`.
fn check_attribute(line: usize, attr: &[Located], failures: &mut Vec<(usize, String)>) {
    let is_cfg_attr = matches!(attr.first(), Some((_, Token::Ident(w))) if w == "cfg_attr");
    let path_key = attr
        .windows(2)
        .any(|w| matches!(w, [(_, Token::Ident(p)), (_, Token::Punct('='))] if p == "path"));
    if is_cfg_attr && path_key {
        failures.push((
            line,
            "a path inside cfg_attr: which file is loaded depends on the cfg; use #[path] alone"
                .to_owned(),
        ));
    }
}
