# ADR 0006: How nondeterminism is kept out of the deterministic core

Status: accepted (flyingrobots/lumen#17). The compiler layer and the dependency rule are in
force; the token scan is the second pull request for the same issue.

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

Echo's patterns missed module imports (`use std::thread; thread::sleep`), module renames, glob
imports, `std::hash::RandomState`, `option_env!`, `Path::exists`, every thread spawn (Echo has no
such rule) and every float form except `f64::NAN`. Clippy missed only what it never compiles or
never sees as a path: three float literals with no float type written (`1.5 as u32`, `1f32`,
`1e3`), a float literal in `format!`, a float and a clock under a `cfg` that is not built, a
`HashMap` in an unused `macro_rules!`, and the comment. Option (2) was not built: it would be a
partial copy of name resolution that Clippy already does exactly, and it would still need the
token-level float and text rules that close Clippy's gaps.

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
  waiver (an `expect` with a reason that starts with a rule ID).

## Scope

The deterministic core is every module of the `lumen` library except its tests: `src/` without
`src/bin/`. It decides the image bytes, the image hash, the scene content digest and the
provenance. Outside it, each for a reason:

- `src/bin/lumen/`: the command reads arguments and files, writes the PNG, times its phases
  (the only clock) and asks the host for the default thread count (the only
  `available_parallelism`). It passes the library values only.
- The scheduler (`src/workers.rs`) starts threads under a waiver: each pixel is written at the
  place its coordinates fix and the sums are integer, so the hand-out order cannot reach the
  image or the ray count (ADR 0003).
- Tests (`tests/`, `tests.rs`) and examples use files, processes and clocks to drive the
  library; a unit test that uses a banned item carries its own waiver.
- `xtask` is repository tooling.

## Consequences

- Not covered: a `usize`-dependent value reaching the hash or an encoder. The defence is the
  WebAssembly leg (ADR 0004), where `usize` is 32 bits and the golden hashes must still match.
- Not covered by the compiler layer: code under a `cfg` that is not built on the CI host, and
  macro bodies that are never invoked. The token scan reads them by name.
- A Clippy waiver covers every ban of its lint on its item, not one rule; the rule IDs in its
  reason say which bans it is meant for.
- A new ban is a line in `clippy.toml`; Clippy warns when a path in it does not resolve.
