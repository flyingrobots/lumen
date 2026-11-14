//! Paths as the scan reads them: every `::` path, `use` trees flattened, and the names `use`
//! declarations bring in.

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

/// The names `use` declarations bring in (the last segment, or the rename) and their paths.
pub(super) fn imports(toks: &[Located]) -> Vec<(String, Vec<String>)> {
    let leaves = (0..toks.len()).filter_map(|i| use_at(toks, i)).flat_map(|(_, leaves)| leaves);
    leaves
        .filter_map(|(path, alias)| {
            let full: Vec<String> = path.into_iter().filter(|s| s != "self").collect();
            Some((alias.or_else(|| full.last().cloned())?, full))
        })
        .collect()
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
