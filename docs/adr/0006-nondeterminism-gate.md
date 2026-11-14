# ADR 0006: How nondeterminism is kept out of the deterministic core

Status: accepted (flyingrobots/lumen#17).

## Context

Lumen claims the same image bytes on every machine. The golden hashes catch a difference only on
the scenes they render. Nothing mechanical stopped a change from bringing a clock, a hash map's
iteration order, the core count or a float into code that decides a pixel.

Echo runs `scripts/ban-nondeterminism.sh`, a grep over source text with rule-scoped waivers;
Bunny replaced its own text gate with rules that resolve every path through the file's `use`
declarations, using the `syn` crate. Lumen takes no crates.io crate, `xtask` included, so it
cannot use `syn`. Three ways were weighed:

1. Echo's grep, ported.
2. A token scanner in `xtask` that rebuilds each file's import map from its `use` trees.
3. The compiler: Clippy's `disallowed-types`, `disallowed-methods` and `disallowed-macros`,
   which resolve paths exactly, through renames, globs, aliases and re-exports.

Each was run on 50 seeded violations and 10 look-alikes: grouped, renamed and glob imports,
module aliases, fully qualified calls, type positions, calls inside `format!`, `vec!` and a local
`macro_rules!`, a float literal in a `const`, a float behind a type alias, `as f64`, `f64::NAN`,
suffix and exponent literals, banned code under a `cfg` that is not built, a banned path in a
macro that is never invoked, and a comment naming open addressing; and as look-alikes, `sin` and
`sqrt` methods on a fixed-point type, `HashMap` and `f64` in comments and strings, identifiers
`instant` and `system_time`, a nested tuple index (`p.0.1`), `1.max(i)`, hex `0xf64` and char
literals. A form counts as caught when its file gets a finding under its rule.

| Approach | Violations caught | Look-alikes rejected |
|---|---|---|
| (1) Echo's patterns | 32 of 50 | 3 of 10 (`sin`/`sqrt`, `HashMap` in a comment, in a string) |
| (3) Clippy with `clippy.toml` | 42 of 50 | 0 of 10 |
| The token scan alone | 46 of 50 | 0 of 10 |
| (3) and the token scan, as decided | 50 of 50 | 0 of 10 |

Echo's patterns missed module imports (`use std::thread; thread::sleep`), module renames, glob
imports, `std::hash::RandomState`, `option_env!`, `Path::exists`, every thread spawn (Echo has no
such rule) and every float form except `f64::NAN`. Clippy missed only what it never compiles or
never sees as a path: three float literals with no float type written (`1.5 as u32`, `1f32`,
`1e3`), a float literal in `format!`, a float and a clock under a `cfg` that is not built, a
`HashMap` in an unused `macro_rules!`, and the comment. Option (2) was not built: it would be a
partial copy of name resolution that Clippy already does exactly, and it would still need the
token-level float and text rules that close Clippy's gaps. The token scan alone misses
`Path::exists` and the three thread spawns, which it does not look for; the scan's forms are its
unit tests (`xtask/src/nondeterminism/tests.rs`).

## Decision

- **The compiler layer.** `clippy.toml` bans the types, functions and macros behind each of
  Echo's rule IDs, plus Lumen's `thread-spawn` (`std::thread::spawn`, `scope`, `Builder`) and
  `float-type` (`f32`, `f64` and Bunny's `FixedQ32_32::to_f32`). Each ban's reason starts with
  its rule ID, so a finding names the rule. `lib.rs` denies `clippy::float_arithmetic`, which
  catches arithmetic on a float whose type is only inferred. The bans run in the existing
  `fmt, clippy` job.
- **Deny by default.** The bans apply to every crate in the workspace. Code outside the
  deterministic core carries `#[expect(<lints>, reason = "<rule-ids>: <why>")]`: the `lumen`
  binary, the examples, `tests/cli.rs` and `xtask` at crate level, and the scheduler and two
  unit tests at the function. `expect`, not `allow`: a waiver that no longer matches anything
  is an `unfulfilled_lint_expectations` error.
- **The dependency rule is `cargo-deny`'s.** `deny.toml` allows no registry, so any crates.io
  crate fails `Gates / cargo-deny` as an unknown registry. That bans `rand`, `getrandom`,
  `serde_json`, `hashbrown`, `tokio`, `wasm-bindgen` and every other crate behind Echo's
  crate-level rules. Bunny's crates, from the one allowed git repository, are checked by
  Bunny's own nondeterminism rules.
- **The token scan** (`cargo run -p xtask -- nondeterminism`) closes Clippy's gaps over the core's
  source text, comments and strings excluded: float types and float literals anywhere, macro
  bodies and `cfg`-gated code included; the banned names (`Instant`, `SystemTime`, `HashMap`,
  `HashSet`, `RandomState`, `available_parallelism`) and `std::env`, `fs`, `process`, `net`
  paths wherever they are spelled; open addressing in any line; and the form of every Clippy
  waiver (an `expect` with a reason that starts with a rule ID, never an `allow`). It resolves a
  path's first segment through the file's `use` trees (groups, `self`, renames, globs), file-wide,
  so it can only over-report. A scan finding is waived by
  `// nondeterminism: allow <rule-id> -- <reason>` on its line or the line above; an unknown
  rule, a missing reason or a waiver that waives nothing fails. It runs as `Gates / nondeterminism`.
- **A waiver is a comment, and nothing else turns a ban off** (flyingrobots/lumen#119). The scan
  takes its waivers from the lexer's line comments, so the same text in a string literal or a
  doc comment waives nothing. Clippy's bans are lints, and a lint-level attribute can silence
  one; each was tried against Clippy 0.1.96 with Lumen's lint levels. The scan checks every
  lint-level attribute in every file, as tokens, so also inside `cfg_attr` and macro bodies, and
  fails any of these: an `allow` or `warn` of a ban lint (`disallowed_methods`,
  `disallowed_types`, `disallowed_macros`, `float_arithmetic`); their renamed names
  `disallowed_method` and `disallowed_type`, which `rustc` still applies; an `allow`, `expect` or
  `warn` of a group that holds one (`clippy::all` and `clippy::style` hold the `disallowed_*`
  lints, `clippy::restriction` holds `float_arithmetic`, per `cargo clippy -- -W help`; and
  `warnings`, which silences a ban lowered to `warn`); a lint list containing a macro
  metavariable (`#[allow(clippy::$lint)]`); and an `expect` of a ban lint whose reason does not
  start with rule IDs. In the core, any attribute that holds a metavariable (`#[$kind(...)]`,
  `#[$meta]`) fails too, since a macro could build a lint attribute from its arguments.
  `allow(warnings)` alone does not silence a ban (they are denied, not warned) and `deny` and
  `forbid` only raise levels; neither is flagged. The group list is the pinned toolchain's; when
  the toolchain moves, re-read `cargo clippy -- -W help` for the groups holding these lints.
- **Both gates read a file as `rustc` lexes it** (flyingrobots/lumen#120, #119). The scan and
  the file-length gate share one lexer (`xtask/src/file_length/count.rs`). It skips a leading
  byte-order mark and a shebang line: a first line starting `#!` whose next token, past
  whitespace and comments that are not doc comments, is not `[` (the reference's rule; `#![...]`
  stays an inner attribute). Without that, `#!/bin/x /*` hid the rest of a file from both gates
  while the compiler built it. Raw identifiers are read without their prefix, so `std::r#env::var`
  is `std::env::var` and `r#f64` is `f64`. A float literal is what the Rust reference's literal
  grammar calls one: a decimal literal with a fraction (`1.5`, `2.`, also after a range's `..`),
  an exponent whose sign may be followed by `_` (`1e-3`, `1e+_3`) or the suffix `f32` or `f64`;
  not `1..2`, `1.foo()`, `2.f64`, `0x1e+3`, `1usize+2` or a tuple index (`t.0.1`). The literal
  forms are unit tests (`xtask/src/nondeterminism/tokens/tests.rs`).

## Scope

The deterministic core is every module of the `lumen` library except its tests. It decides the
image bytes, the image hash, the scene content digest and the provenance. `xtask/src/layout.rs`
finds it by following the library's module tree from `src/lib.rs` the way `rustc` resolves `mod`
declarations (`foo.rs` or `foo/mod.rs`, `#[path]` relative to the declaring file, inline
modules), not by file names: every file the library compiles outside `#[cfg(test)]` is core,
wherever it is (a `#[path]` into `tests/` included), and so is any file under `src/` outside
`src/bin/` that the module tree does not reach, or reaches only through the command. A file is a
test file only when every declaration that reaches it is under `#[cfg(test)]` (on the `mod`
item, an enclosing inline module, or the file's own `#![cfg(test)]`), or when no crate root
reaches it and it is under the crate's `tests/`; a module named `tests.rs`, or under
`src/tests/`, without `cfg(test)` is core. The file-length gate takes its 300/500 and 750
limits from the same roles. Only `cfg(test)` written exactly so counts: `cfg(all(test, x))` is
read as source, which can only over-report.

Both gates read only `.rs` files under `crates/` and `xtask/`, so they fail on what would make
the compiler read anything else: `include!` in any file (it is not needed: a module does the
same and is read), a `#[path]` naming a file that is not such a `.rs` file (a `.inc` file, a
missing file, or a path leaving the repository), and a `path` inside `cfg_attr` (which file it
loads depends on the `cfg`). `#[path]` takes only a string literal (`rustc` refuses
`#[path = concat!(..)]`), so no macro can build the path, and the rule holds at every level of
indirection because each loaded file is itself walked. `include_str!` and `include_bytes!` read
data, not code, and stay allowed (the library embeds its PLY meshes with them). Not covered:
code a build script generates (there is none; one would have to be `include!`d, which fails) and
`mod` declarations that a macro builds from its arguments (`mod $name;`), which are resolved
relative to the macro's file, not the caller's. Outside the core, each for a reason:

- `src/bin/lumen/`: the command reads arguments and files, writes the PNG, times its phases
  (the only clock) and asks the host for the default thread count (the only
  `available_parallelism`). It passes the library values only.
- The scheduler (`src/workers.rs`) starts threads under a waiver: each pixel is written at the
  place its coordinates fix and the sums are integer, so the hand-out order cannot reach the
  image or the ray count (ADR 0003).
- Tests (integration tests and modules under `#[cfg(test)]`) and examples use files, processes
  and clocks to drive the library; a unit test that uses a banned item carries its own waiver.
- `xtask` is repository tooling.

## Consequences

- Not covered: a `usize`-dependent value reaching the hash or an encoder. The defence is the
  WebAssembly leg (ADR 0004), where `usize` is 32 bits and the golden hashes must still match.
- Not covered by the compiler layer: code under a `cfg` that is not built on the CI host, and
  macro bodies that are never invoked. The token scan reads them by name.
- A Clippy waiver covers every ban of its lint on its item, not one rule; the rule IDs in its
  reason say which bans it is meant for.
- A new ban is a line in `clippy.toml`. A path that does not resolve is only a warning about
  `clippy.toml` itself, which `-D warnings` does not turn into an error: read Clippy's output
  when adding one.
