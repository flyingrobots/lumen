//! This module owns the scan's token rules for values that depend on the host, the schedule or
//! the address-space layout and that no path names (flyingrobots/lumen#155): macros, method
//! calls by name, pointer-to-integer casts, `{:p}` formats, platform `cfg`s and `static` items.

use super::tokens::{Located, Token};

/// Macros, by name, and their rules. `dbg!` prints to standard error with the source path.
const MACROS: [(&str, &str); 9] = [
    ("thread_local", "shared-state"),
    ("file", "source-path"),
    ("dbg", "host-io"),
    ("print", "host-io"),
    ("println", "host-io"),
    ("eprint", "host-io"),
    ("eprintln", "host-io"),
    ("cfg", "platform-branch"),
    ("cfg_select", "platform-branch"),
];

/// Methods, by name, and their rules: a pointer's address (`addr`, `expose_provenance`) or a
/// property of it (`is_aligned`, `align_offset`), and whether a stream is a terminal. The
/// receiver's type is not known, so a method of the core's own with one of these names is a
/// finding too.
const METHODS: [(&str, &str); 7] = [
    ("addr", "address"),
    ("expose_provenance", "address"),
    ("expose_addr", "address"),
    ("align_offset", "address"),
    ("is_aligned", "address"),
    ("is_aligned_to", "address"),
    ("is_terminal", "host-io"),
];

/// The `cfg` predicates that do not depend on the target, the host or the profile: `test`,
/// `doc` and `doctest` are how the code is built for checking, and `feature` is the caller's
/// choice, written in its manifest.
const NEUTRAL_CFGS: [&str; 7] = ["all", "any", "not", "test", "feature", "doc", "doctest"];

/// The integer types a pointer can be cast to.
const INTEGERS: [&str; 12] =
    ["usize", "isize", "u8", "u16", "u32", "u64", "u128", "i8", "i16", "i32", "i64", "i128"];

/// The pointer-making functions whose result cast to an integer is an address.
const POINTER_CALLS: [&str; 6] =
    ["as_ptr", "as_mut_ptr", "from_ref", "from_mut", "addr_of", "cast"];

/// The rule under which the scan refuses the macro `name` (`file`, `cfg`, `is_x86_feature_detected`,
/// `env`), if it does.
pub(super) fn macro_rule(name: &str) -> Option<&'static str> {
    if let Some((_, rule)) = MACROS.iter().find(|(m, _)| *m == name) {
        Some(rule)
    } else if name.starts_with("is_") && name.ends_with("_feature_detected") {
        Some("host-cpu")
    } else {
        (name == "env" || name == "option_env").then_some("std-env")
    }
}

/// Each host finding of `toks`: its line, rule and what was found.
pub(super) fn host_findings(toks: &[Located]) -> Vec<(usize, &'static str, String)> {
    let mut out = Vec::new();
    let at = |i: usize| toks.get(i).map(|(_, t)| t);
    for (i, (line, token)) in toks.iter().enumerate() {
        match token {
            Token::Ident(name) if at(i + 1) == Some(&Token::Punct('!')) && opens(at(i + 2)) => {
                match macro_rule(name) {
                    Some("platform-branch") if neutral(&toks[i + 2..]) => {}
                    Some("std-env") | None => {}
                    Some(rule) => out.push((*line, rule, format!("{name}!"))),
                }
            }
            Token::Ident(name)
                if i.checked_sub(1).and_then(at) == Some(&Token::Punct('.'))
                    && at(i + 1) == Some(&Token::Punct('(')) =>
            {
                if let Some((_, rule)) = METHODS.iter().find(|(m, _)| m == name) {
                    out.push((*line, *rule, format!(".{name}()")));
                }
            }
            Token::Ident(word) if word == "static" && is_static_item(toks, i) => {
                out.push((
                    *line,
                    "shared-state",
                    "a static item: state outside the pixel function's inputs".to_owned(),
                ));
            }
            Token::Ident(word) if word == "as" && pointer_cast(toks, i) => {
                out.push((*line, "address", "a pointer cast to an integer".to_owned()));
            }
            Token::Punct('#') => {
                if let Some(what) = platform_attribute(toks, i) {
                    out.push((*line, "platform-branch", what));
                }
            }
            Token::Str(text) if formats_pointer(text) => {
                out.push((*line, "address", "a {:p} format".to_owned()));
            }
            _ => {}
        }
    }
    out
}

/// Whether a token opens a macro's arguments.
fn opens(token: Option<&Token>) -> bool {
    matches!(token, Some(Token::Punct('(' | '[' | '{')))
}

/// Whether the bracketed arguments at the start of `toks` name only neutral `cfg` predicates
/// ([`NEUTRAL_CFGS`]): every identifier in them is one, strings aside (`feature = "x"`).
fn neutral(toks: &[Located]) -> bool {
    bracketed(toks)
        .iter()
        .all(|(_, t)| !matches!(t, Token::Ident(w) if !NEUTRAL_CFGS.contains(&w.as_str())))
}

/// The tokens inside the bracket that opens `toks`, up to its match.
fn bracketed(toks: &[Located]) -> &[Located] {
    if !opens(toks.first().map(|(_, t)| t)) {
        return &[];
    }
    let mut depth = 0_usize;
    for (k, (_, t)) in toks.iter().enumerate() {
        match t {
            Token::Punct('(' | '[' | '{') => depth += 1,
            Token::Punct(')' | ']' | '}') => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    return &toks[1..k];
                }
            }
            _ => {}
        }
    }
    toks.get(1..).unwrap_or(&[])
}

/// What is wrong with the attribute whose `#` is at `i`, if it is `cfg(...)` or the predicate of
/// `cfg_attr(...)` and names a predicate that is not neutral.
fn platform_attribute(toks: &[Located], i: usize) -> Option<String> {
    let at = |k: usize| toks.get(k).map(|(_, t)| t);
    let open = i + 1 + usize::from(at(i + 1) == Some(&Token::Punct('!')));
    if at(open) != Some(&Token::Punct('[')) {
        return None;
    }
    let Some(Token::Ident(kind)) = at(open + 1) else { return None };
    let args = bracketed(toks.get(open + 2..)?);
    let predicate: &[Located] = match kind.as_str() {
        "cfg" => args,
        "cfg_attr" => {
            let mut depth = 0_usize;
            let end = args.iter().position(|(_, t)| {
                match t {
                    Token::Punct('(' | '[') => depth += 1,
                    Token::Punct(')' | ']') => depth = depth.saturating_sub(1),
                    _ => {}
                }
                depth == 0 && *t == Token::Punct(',')
            });
            &args[..end.unwrap_or(args.len())]
        }
        _ => return None,
    };
    let named = predicate.iter().find_map(|(_, t)| match t {
        Token::Ident(w) if !NEUTRAL_CFGS.contains(&w.as_str()) => Some(w.clone()),
        _ => None,
    })?;
    Some(format!("#[{kind}({named} ...)]: a branch on the target, the host or the profile"))
}

/// Whether the `static` at `i` starts a `static` item (`static NAME:`, `static mut NAME:`),
/// not a `'static` lifetime.
fn is_static_item(toks: &[Located], i: usize) -> bool {
    let at = |k: usize| toks.get(k).map(|(_, t)| t);
    if i.checked_sub(1).and_then(at) == Some(&Token::Punct('\'')) {
        return false;
    }
    let name = i + 1 + usize::from(matches!(at(i + 1), Some(Token::Ident(m)) if m == "mut"));
    matches!(at(name), Some(Token::Ident(_))) && at(name + 1) == Some(&Token::Punct(':'))
}

/// Whether the `as` at `i` casts a pointer to an integer: `as *const T as usize`, or
/// `x.as_ptr() as usize` and the like ([`POINTER_CALLS`]).
fn pointer_cast(toks: &[Located], i: usize) -> bool {
    let at = |k: usize| toks.get(k).map(|(_, t)| t);
    let integer =
        |k: usize| matches!(at(k), Some(Token::Ident(w)) if INTEGERS.contains(&w.as_str()));
    if at(i + 1) == Some(&Token::Punct('*'))
        && matches!(at(i + 2), Some(Token::Ident(w)) if w == "const" || w == "mut")
    {
        let mut k = i + 3;
        while let Some(t) = at(k) {
            match t {
                Token::Ident(w) if w == "as" => return integer(k + 1),
                Token::Ident(_) | Token::Punct(':' | '<' | '>' | '*' | '_') => k += 1,
                _ => return false,
            }
        }
        return false;
    }
    if !integer(i + 1) || i.checked_sub(1).and_then(at) != Some(&Token::Punct(')')) {
        return false;
    }
    let mut depth = 0_usize;
    for k in (0..i).rev() {
        match at(k) {
            Some(Token::Punct(')')) => depth += 1,
            Some(Token::Punct('(')) => {
                depth -= 1;
                if depth == 0 {
                    return matches!(k.checked_sub(1).and_then(at), Some(Token::Ident(w)) if POINTER_CALLS.contains(&w.as_str()));
                }
            }
            _ => {}
        }
    }
    false
}

/// Whether a string literal holds a `{:p}` format (`{x:p}`, `{:#p}`, `{0:>16p}`), which prints
/// an address. `{{` is an escaped brace.
fn formats_pointer(text: &str) -> bool {
    text.replace("{{", "").split('{').skip(1).any(|rest| {
        let spec = rest.split('}').next().unwrap_or_default();
        spec.split_once(':').is_some_and(|(_, format)| {
            format.strip_suffix('p').is_some_and(|flags| {
                flags.chars().all(|c| c.is_ascii_digit() || "#<>^+-.$".contains(c))
            })
        })
    })
}
