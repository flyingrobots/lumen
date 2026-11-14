//! Paths as the scan reads them: every `::` path, `use` trees flattened, the names `use`
//! declarations bring in, and the aliases of all the deterministic crates' files together
//! ([`CrateAliases`]), so an alias declared in one file is followed in another.

use super::tokens::{Located, Token};

/// Every `::` path in the tokens with its line, `use` trees flattened into full paths.
pub(super) fn paths(toks: &[Located]) -> Vec<(usize, Vec<String>)> {
    let mut out = Vec::new();
    let mut i = 0;
    while let Some((line, _)) = toks.get(i) {
        if let Some((end, leaves)) = use_at(toks, i) {
            out.extend(leaves.into_iter().map(|(p, _)| (*line, p)));
            i = end;
            continue;
        }
        let mut path = Vec::new();
        let mut j = i;
        while let Some((_, Token::Ident(segment))) = toks.get(j) {
            path.push(segment.clone());
            if !matches!(
                (toks.get(j + 1), toks.get(j + 2)),
                (Some((_, Token::Punct(':'))), Some((_, Token::Punct(':'))))
            ) {
                break;
            }
            j += 3;
        }
        if path.len() > 1 {
            out.push((*line, path));
        }
        i = j.max(i) + 1;
    }
    out
}

/// A name and the path it stands for.
pub(super) type Import = (String, Vec<String>);

/// The aliases every file of the deterministic crates can reach: the `extern crate ... as`
/// names, which are in scope everywhere when declared at a crate root (all are taken, wherever
/// declared), every `use` name of every file, reachable by a path through the crate's modules
/// (`crate::probe_b::reexported`), and the deterministic crates' names, which start such paths
/// from another crate. Over-approximate by design: the scan can only over-report.
#[derive(Clone, Debug, Default)]
pub(crate) struct CrateAliases {
    externs: Vec<Import>,
    uses: Vec<Import>,
    crates: Vec<String>,
}

/// The aliases of `texts`, the deterministic crates' core files, whose crate names are `crates`.
pub(crate) fn crate_aliases(texts: &[&str], crates: &[String]) -> CrateAliases {
    let mut aliases = CrateAliases { crates: crates.to_vec(), ..CrateAliases::default() };
    for text in texts {
        let toks = super::tokens::tokens(text);
        aliases.externs.extend(extern_aliases(&toks));
        aliases.uses.extend(use_imports(&toks));
    }
    for list in [&mut aliases.externs, &mut aliases.uses] {
        list.sort();
        list.dedup();
    }
    aliases
}

/// The names `use` declarations and `extern crate ... as` items bring in (the last segment, or
/// the rename) and their paths.
pub(super) fn imports(toks: &[Located]) -> Vec<Import> {
    let mut out = use_imports(toks);
    out.extend(extern_aliases(toks));
    out
}

/// The names `use` declarations bring in and their paths.
fn use_imports(toks: &[Located]) -> Vec<Import> {
    let leaves = (0..toks.len()).filter_map(|i| use_at(toks, i)).flat_map(|(_, leaves)| leaves);
    leaves
        .filter_map(|(path, alias)| {
            let full: Vec<String> = path.into_iter().filter(|s| s != "self").collect();
            Some((alias.or_else(|| full.last().cloned())?, full))
        })
        .collect()
}

/// The names `extern crate <krate> as <alias>` items bring in.
fn extern_aliases(toks: &[Located]) -> Vec<Import> {
    let mut out = Vec::new();
    for window in toks.windows(5) {
        let words: Vec<&Token> = window.iter().map(|(_, t)| t).collect();
        if let [Token::Ident(e), Token::Ident(c), Token::Ident(krate), Token::Ident(keyword), Token::Ident(alias)] =
            words.as_slice()
        {
            if e == "extern" && c == "crate" && keyword == "as" && alias != "_" {
                out.push((alias.clone(), vec![krate.clone()]));
            }
        }
    }
    out
}

/// Every reading of `path` through the file's `imports` and the crates' `aliases`: the path
/// itself, each path its first segment's imports or `extern crate` aliases make of it, and, for
/// a path through the crates' own modules (from `crate`, `self`, `super` or a deterministic
/// crate's name), each path a later segment's `use` name makes of it
/// (`crate::probe_b::reexported::fs` is `std::fs` when any file has `pub use std as
/// reexported`), repeatedly (`extern crate std as host; use host::env as e;` makes `e::var`
/// `std::env::var`), at most eight steps and 256 readings. Resolution is not by module, so where
/// modules bind one name differently, every binding is tried: the scan can only over-report.
pub(super) fn resolve(
    path: &[String],
    imports: &[Import],
    aliases: &CrateAliases,
) -> Vec<Vec<String>> {
    let first: Vec<&Import> = imports.iter().chain(&aliases.externs).collect();
    let later: Vec<&Import> = imports.iter().chain(&aliases.uses).chain(&aliases.externs).collect();
    let mut readings = vec![path.to_vec()];
    let mut frontier = vec![path.to_vec()];
    for _ in 0..8 {
        let mut next = Vec::new();
        for reading in &frontier {
            let mut add = |resolved: Vec<String>| {
                if !readings.contains(&resolved) && readings.len() < 256 {
                    readings.push(resolved.clone());
                    next.push(resolved);
                }
            };
            for (_, full) in first.iter().filter(|(alias, _)| reading.first() == Some(alias)) {
                add([&full[..], &reading[1..]].concat());
            }
            let root = reading.first().map_or("", String::as_str);
            if ["crate", "self", "super"].contains(&root)
                || aliases.crates.iter().any(|c| c == root)
            {
                for (k, segment) in reading.iter().enumerate().skip(1) {
                    for (_, full) in later.iter().filter(|(alias, _)| alias == segment) {
                        add([&full[..], &reading[k + 1..]].concat());
                    }
                }
            }
        }
        frontier = next;
    }
    readings
}

/// Every path in the tokens with a segment that is a macro metavariable (`std::$m::var`,
/// `$root::fs::read`), with its line; a metavariable segment is `$` and its name. `$crate` is
/// not one: [`paths`] reads `$crate::x` as `crate::x`.
pub(super) fn macro_paths(toks: &[Located]) -> Vec<(usize, Vec<String>)> {
    let segment = |i: usize| match (toks.get(i), toks.get(i + 1)) {
        (Some((_, Token::Punct('$'))), Some((_, Token::Ident(name)))) if name != "crate" => {
            Some((format!("${name}"), 2))
        }
        (Some((_, Token::Ident(name))), _) => Some((name.clone(), 1)),
        _ => None,
    };
    let colons = |i: usize| {
        matches!(
            (toks.get(i), toks.get(i + 1)),
            (Some((_, Token::Punct(':'))), Some((_, Token::Punct(':'))))
        )
    };
    let mut out = Vec::new();
    let mut i = 0;
    while let Some((line, _)) = toks.get(i) {
        let (mut path, mut j) = (Vec::new(), i);
        while let Some((name, width)) = segment(j) {
            path.push(name);
            j += width;
            if !colons(j) {
                break;
            }
            j += 2;
        }
        if path.len() > 1 && path.iter().any(|s| s.starts_with('$')) {
            out.push((*line, path));
        }
        i = j.max(i + 1);
    }
    out
}

/// If a `use` declaration starts at `i`: the index of its `;` and its leaves ([`use_tree`]).
fn use_at(toks: &[Located], i: usize) -> Option<(usize, Vec<Leaf>)> {
    if toks.get(i)?.1 != Token::Ident("use".to_owned()) {
        return None;
    }
    let end =
        toks[i..].iter().position(|(_, t)| *t == Token::Punct(';')).map_or(toks.len(), |e| i + e);
    Some((end, use_tree(&toks[i + 1..end])))
}

/// A leaf of a `use` tree: its full path and its rename, if it has one.
type Leaf = (Vec<String>, Option<String>);

/// The leaves of a `use` tree.
fn use_tree(toks: &[Located]) -> Vec<Leaf> {
    let (mut out, mut stack, mut path) = (Vec::new(), Vec::new(), Vec::<String>::new());
    let mut alias = None;
    let mut iter = toks.iter().map(|(_, t)| t);
    while let Some(token) = iter.next() {
        match token {
            Token::Ident(word) if word == "as" => {
                alias = iter.next().and_then(|t| {
                    if let Token::Ident(a) = t {
                        Some(a.clone())
                    } else {
                        None
                    }
                });
            }
            Token::Ident(word) => path.push(word.clone()),
            Token::Punct('*') => path.push("*".to_owned()),
            Token::Punct('{') => stack.push(path.len()),
            Token::Punct(',' | '}') => {
                if path.len() > stack.last().copied().unwrap_or(0) {
                    out.push((path.clone(), alias.take()));
                }
                let keep =
                    if *token == Token::Punct('}') { stack.pop() } else { stack.last().copied() };
                path.truncate(keep.unwrap_or(0));
            }
            _ => {}
        }
    }
    if !path.is_empty() {
        out.push((path, alias));
    }
    out
}
