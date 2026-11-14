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
unit tests (`xtask/src/nondeterminism/tests.rs`). Clippy's 42, with the six forms of
flyingrobots/lumen#115 it catches, are the seeded forms of `xtask/clippy-fixture/src/forms.rs`,
and its 10 look-alikes are in `look_alikes.rs`; `cargo run -p xtask -- clippy-bans` re-runs
them in CI (see Consequences), so this row is a check, not a recorded run.

## Decision

- **The compiler layer.** `clippy.toml` bans the types, functions and macros behind each of
  Echo's rule IDs, plus Lumen's `thread-spawn` (`std::thread::spawn`, `scope`, `Builder`) and
  `float-type`. Each ban's reason starts with its rule ID, so a finding names the rule. Clippy
  matches by resolved path, so an alias, a re-export or a raw identifier does not matter to it,
  but it bans only what is listed (flyingrobots/lumen#115 found `host::fs::exists`,
  `std::r#fs::copy` and `sleep_ms` unlisted), so the lists are complete by module: for
  `std::env`, `std::fs`, `std::process` and `std::net`, every stable function and type in the
  pinned toolchain's documentation (`std/all.html` of rust-docs 1.96.0, stability read from each
  item's page), except the address types of `std::net` (`IpAddr`, `SocketAddr` and the like:
  core's plain data, no socket and no lookup; `ToSocketAddrs`, which looks names up, is banned);
  of `std::thread`, the sleeps (`sleep`, `sleep_ms`, `park_timeout`, `park_timeout_ms`), the
  spawns and `available_parallelism`, not `current`, `park`, `yield_now`, `JoinHandle` and the
  rest that the scheduler may use; of `std::time`, `SystemTime`, `SystemTimeError` and
  `Instant`. Unstable items cannot be used on the pinned stable toolchain and are not listed.
  `float-type` also covers every stable `Duration` method that takes or returns a float
  (`as_secs_f64`, `mul_f64`, `from_secs_f64`, `try_from_secs_f64`, `div_duration_f64` and their
  `f32` forms) and Bunny's float bridge at the pinned commit: `bunny_num::Scalar`,
  `FixedQ32_32::{from_f32, to_f32, try_from_f32}` and their `conversions` functions,
  `bunny_linalg::{Vec2, Vec3}` and `FixedVec{2,3}::try_from_float`, `bunny_geom::{Ray3, Aabb3,
  Sphere3}` and the `into_float`/`try_from_float` of their fixed forms, and `bunny_codec`'s float
  meshes (`ObjMesh`, `ObjVertex`, `ObjVertices`, `PlyBinaryMesh`, `PlyVertex`, `parse_obj_text`,
  `parse_binary_ply`), whose `pub x: f32` fields carry a float with no token written. Constants
  cannot be banned by Clippy (`std::env::consts::OS`, `UNIX_EPOCH`); they are the token scan's.
  `lib.rs` denies `clippy::float_arithmetic`, which catches `+ - * / %` and negation on a float
  whose type is only inferred (not method calls, comparisons or conversions). The bans run in
  the existing `fmt, clippy` job.
- **The std part of the lists is derived, not remembered.** `xtask/bans/std-items.txt`, checked
  in, holds every public item of `std::env`, `fs`, `process`, `net`, `thread` and `time` and
  every float method of `Duration`, with its stability, for the pinned toolchain.
  `xtask/src/clippy_bans/std_items.rs` decides each one (a module rule, the kept lists of
  `std::net` and `std::thread`, unstable items and constants kept), and
  `cargo run -p xtask -- clippy-bans` fails when an item decided banned has no `clippy.toml`
  entry under its rule, when an item has no decision (a new `std::thread` or `std::time`
  item), when a row is not `<kind> std::<path> stable|unstable` or the list has no
  `toolchain` line, and when the list's toolchain is not the one `rust-toolchain.toml` pins
  (`std-items` stops on a documentation page it cannot read rather than guess). When the
  toolchain moves, regenerate the list from that toolchain's documentation and commit it:

  ```sh
  T=rust-docs-<version>-x86_64-unknown-linux-gnu
  curl -sSfO https://static.rust-lang.org/dist/$T.tar.xz
  curl -sSfO https://static.rust-lang.org/dist/$T.tar.xz.sha256 && sha256sum -c $T.tar.xz.sha256
  tar -xJf $T.tar.xz $T/rust-docs/share/doc/rust/html/std
  cargo run --locked -q -p xtask -- std-items $T/rust-docs/share/doc/rust/html/std <version> \
    > xtask/bans/std-items.txt
  ```

  (in the container, with `--network`), then decide every new item and add each new ban to
  `clippy.toml` and a use of it to the fixture, until `clippy-bans` passes. The list of 1.96.0
  is from the tarball with sha256
  `75a89ce0e81e67d0134bd7adb1f0e312110b8eaef2fdd07e3bfb072a7211317b`, and regenerating it
  reproduces the checked-in file byte for byte. Bunny's float bridge is listed by hand from its
  sources at the pinned commit; when the pin moves, re-read them.
- **Deny by default.** The bans apply to every crate in the workspace. Code outside the
  deterministic core carries `#[expect(<lints>, reason = "<rule-ids>: <why>")]`: the `lumen`
  binary, the examples, `tests/cli.rs` and `xtask` at crate level, and the scheduler and two
  unit tests at the function. `expect`, not `allow`: a waiver that no longer matches anything
  is an `unfulfilled_lint_expectations` error.
- **The dependency rule is `cargo-deny`'s, and the workspace boundary's.** `deny.toml` allows no
  registry, so any crates.io crate fails `Gates / cargo-deny` as an unknown registry. That bans
  `rand`, `getrandom`, `serde_json`, `hashbrown`, `tokio`, `wasm-bindgen` and every other crate
  behind Echo's crate-level rules. Bunny's crates, from the one allowed git repository, are
  checked by Bunny's own nondeterminism rules. `cargo-deny` does not check path dependencies;
  the workspace boundary does (see the amendment below).
- **The token scan** (`cargo run -p xtask -- nondeterminism`) closes Clippy's gaps over the core's
  source text, comments and strings excluded: float types and float literals anywhere, macro
  bodies and `cfg`-gated code included; the banned names (`Instant`, `SystemTime`, `HashMap`,
  `HashSet`, `RandomState`, `available_parallelism`) and `std::env`, `fs`, `process`, `net`
  paths wherever they are spelled; open addressing in any line; and the form of every Clippy
  waiver (an `expect` with a reason that starts with a rule ID, never an `allow`). It resolves a
  path's first segment through the file's `use` trees (groups, `self`, renames, globs) and
  `extern crate ... as` aliases, repeatedly (`extern crate std as host; use host::env as e;`
  makes `e::var` a `std-env` finding), file-wide, so it can only over-report. The prefix rule
  bans whole modules (`std::env`, `fs`, `process`, `net`), so it also catches members
  `clippy.toml` does not list (`host::fs::exists`, `host::env::set_current_dir`,
  `host::env::consts::OS`); the `std::thread` sleeps are listed by name (`sleep`, `sleep_ms`,
  `sleep_until`, `park_timeout`, `park_timeout_ms`), since a prefix of whole segments does not
  make `sleep_ms` a `sleep`. A name with the word `f32` or `f64` (`as_secs_f64`, `mul_f32`,
  `from_secs_f64`, Bunny's `to_f32`) is `float-type`: a float passes through such a call even
  when no float type or literal is written (`d.mul_f64(d.as_secs_f64())`). A scan finding is waived by
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

The deterministic core is every module of the deterministic crates' libraries except their
tests: `lumen` and every workspace member its package depends on (the amendment of 2026-11-14
below derives them from `cargo metadata`; there is none besides `lumen` today). It decides the
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
code a build script generates (a deterministic crate may have no build script, see the
amendment) and
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
- Not seen by either layer: a float whose type is only inferred, reached through an API that
  neither list names and whose name has no `f16`, `f32`, `f64`, `f128` or `_float` (a generic
  `T: From<u32>` instantiated by inference, or a float API a new dependency brings). A syntactic
  gate cannot see an inferred type. The defences: no float type can be written in the core under
  the names the scan knows, which are every name std gives one (`f16`, `f32`, `f64`, `f128`,
  also as `core::primitive::f64`, and the aliases `c_double` and `c_float` of `core::ffi`,
  `std::ffi` and `std::os::raw`; amendment of 2026-11-14), and an alias the core or a
  deterministic crate defines is a finding where it is defined, since its definition names one
  of them; no float literal can be written; every
  float-taking and float-returning API the library can name today is listed, `float_arithmetic`
  catches arithmetic on such a float, and a new dependency fails `Gates / cargo-deny` until
  `deny.toml` is changed on purpose, which is when its float API must be listed.
- A Clippy waiver covers every ban of its lint on its item, not one rule; the rule IDs in its
  reason say which bans it is meant for.
- A new ban is a line in `clippy.toml` and a use of the banned item in `xtask/clippy-fixture`.
  Clippy reports a path that does not resolve only as a warning about `clippy.toml` itself,
  which `-D warnings` does not turn into an error (a misspelt `Path::exits` left `p.exists()`
  unbanned, and `cargo clippy` exited 0), and a `clippy.toml` or `.clippy.toml` in a crate's
  directory replaces the root one for everything under it (a one-line `xtask/.clippy.toml`
  turned off all 53 bans there). So `cargo run -p xtask -- clippy-bans`, a step of the
  `fmt, clippy` job, runs Clippy on the fixture and fails on: any item of a `disallowed-*`
  array it cannot read as `{ path = "...", reason = "<rule-id>: ..." }` (a bare string, a
  missing reason, any other key such as `allow-invalid`, which silences the warning below), so
  a ban it cannot check is a failure, not a pass; any diagnostic located in `clippy.toml`; an entry that never fires on the fixture (misspelt, resolving to another item,
  or not used there: this is the stronger of the two checks, since it does not depend on the
  wording of Clippy's warning); a module marked `// seeded: <rule>` with no finding under that
  rule, a finding in a module marked `// accepted` or outside every marked module; a fixture
  that does not compile; any Clippy configuration file other than the root `clippy.toml`, and
  `CLIPPY_CONF_DIR`; and a toolchain whose groups holding a ban lint (`cargo clippy -- -W help`)
  differ from the ones the scan refuses. The fixture is a workspace member whose code is behind
  `--cfg lumen_clippy_fixture`, which only the check passes, so it is linted with the
  workspace's own lint levels and lock file, and ordinary builds compile an empty crate. It costs
  the `fmt, clippy` job one `xtask` build and one Clippy run over a single small crate. It
  checks that each listed ban works, not that the list is complete. Lint levels set outside
  source files are the workspace boundary's (amendment of 2026-11-14): a member's own `[lints]`
  table and any `.cargo/config.toml` fail; `RUSTFLAGS` in the builder's own environment is not
  read by any gate.

## Amendment, 2026-11-14: the workspace boundary

flyingrobots/lumen#158 found two ways for code the gates do not read to decide the library: a
second workspace crate as a path dependency (floats and `sin`, no waiver, and neither layer
looked at it), and a build script that sets a `cfg` the library branches on.

- **The deterministic crates are derived, not listed.** `xtask nondeterminism` runs
  `cargo metadata --no-deps --format-version 1` and reads every workspace member and its
  declared dependencies (`xtask/src/workspace.rs`, with a JSON reader of its own, since `xtask`
  takes no crates.io crate). The deterministic crates are `lumen` and every member its package
  reaches through normal and build dependencies, on every target and with every optional
  dependency, since the metadata lists them all whatever the platform or features. Dev
  dependencies are not followed: they build only tests. Every other member must be classified
  as tooling, with its reason, in `TOOLING` (`xtask` and `lumen-clippy-fixture` today); a
  member that is neither fails, and so does a tooling crate the library comes to depend on.
- **What the boundary refuses**, for every member: a path dependency whose directory is not a
  workspace member (cargo makes every path dependency inside the workspace a member, so this is
  one outside it), and a dependency from any source but Bunny's repository at a full 40-digit
  commit (the rule `deny.toml` holds, repeated because `cargo-deny` reads only the resolved graph
  and not path dependencies). For a deterministic crate: a build script (a `custom-build`
  target, whether the file is `build.rs` or named by `build =`), since it can set a `cfg` or an
  environment variable the library reads (`cargo::rustc-cfg`, `cargo::rustc-env`) with no
  `include!`; and a procedural macro, whose output is code the gates never read. No
  deterministic crate needs either today. The fixture's `build.rs`, which only declares its
  `cfg`, is tooling's.
- **Every deterministic crate is core.** The module walk starts at each deterministic crate's
  library root, and the scan reads every file it reaches outside `#[cfg(test)]` as core, plus
  any other file under that crate's `src/` outside `src/bin/`, as it always did for `lumen`.
  The gates read only `crates/` and `xtask/`, so a deterministic crate elsewhere fails.
- **`clippy::float_arithmetic` is denied at each deterministic crate's root**, as an inner
  `#![deny(...)]` or `#![forbid(...)]` that names it; the gate fails a root without one. It is
  not a workspace lint because the binary, the tests and the tooling do float arithmetic (the
  command's timings, `tests/cli.rs`, the examples), and a workspace lint would need a waiver in
  each. `clippy.toml`'s bans already apply to every crate.
- **Lint levels and flags outside source files.** Every member's manifest must have `[lints]`
  with `workspace = true` alone and no `[lints.*]` table of its own (one lowered
  `disallowed_methods` in no file the scan reads; the workspace's own `[workspace.lints]` is
  checked by the fixture, which inherits it). The root manifest may have no `[patch]` or
  `[replace]` table, which would swap a Bunny crate for code the gates do not read. And the
  repository may hold no Cargo configuration file (`.cargo/config.toml` or `.cargo/config`, at
  any depth, since Cargo reads them from the directory it runs in upwards): one can set
  `rustflags` with a `--cfg` the core branches on, or `-A` of a ban lint, and nothing needs
  one. A `cfg` the core names that no build script or `check-cfg` declares is an
  `unexpected_cfgs` error, which `warnings = "deny"` makes fatal.
- Failures print as `[workspace]` errors of `Gates / nondeterminism`, the root check as
  `[float-type]`.

## Amendment, 2026-11-14: float types under other names

flyingrobots/lumen#156 found `<core::ffi::c_double as From<u32>>::from(x).sin()` in the core,
passing both layers: `c_double` is `f64` under another name, so no float token is written,
`disallowed-types` banned the path `f64` and not the alias, and `float_arithmetic` sees no
arithmetic operator. `c_float` was caught only because its name ends `_float`.

- **Clippy** bans `core::ffi::c_double` and `core::ffi::c_float`, and `std::os::raw::c_double`
  and `std::os::raw::c_float` (`float-type`). `std::ffi` re-exports the `core::ffi` aliases, so
  those two entries catch it and any rename; `std::os::raw` defines aliases of its own, which
  the `core::ffi` entries do not catch (checked with Clippy 0.1.96), hence four entries. The
  fixture's forms spell all three.
- **The scan** names `c_double` and `c_float` wherever they are spelled, and treats `f16` and
  `f128` as float types like `f32` and `f64`, also as words of a longer name (`to_f16`). Both are
  unstable on 1.96.0, so Clippy cannot ban them, and naming them keeps a toolchain that
  stabilises them from bringing one in unseen. `core::primitive::f64` already ended in the token
  `f64`.
- An alias of a float type written in the core or in any deterministic crate
  (`type Real = core::ffi::c_double;`) is a finding at its definition, which names a float type;
  a use of the alias elsewhere is not a second finding, and need not be. std has no other float
  alias, and Bunny's float bridge is listed by name.

## Amendment, 2026-11-14: aliases across files, macro-built paths and `cfg`

flyingrobots/lumen#157 found paths that pass both layers: an alias declared in one file
(`extern crate std as host;` at the crate root, `pub use std as reexported;` in another module)
used in another file under a `cfg` the Linux Clippy run does not build (`cfg(windows)`,
`cfg(target_arch = "wasm32")`), and a path a macro builds from its argument (`std::$m::var_os`).
Clippy caught the forms it compiled; the scan resolved aliases only within one file.

- **Aliases are followed across the deterministic crates' files.** Before scanning, the gate
  collects the aliases of every core file (`CrateAliases` in `xtask/src/nondeterminism/paths.rs`):
  every `extern crate ... as` name, which a crate root puts in every module's scope (all are
  taken, wherever declared), and every `use` name of every file. A path's first segment resolves
  through its own file's imports and the `extern crate` names; a path through the crates' own
  modules (starting `crate`, `self`, `super`, `$crate` or a deterministic crate's name) also
  resolves through any later segment that is a `use` name of any file, so
  `crate::probe_b::reexported::fs::metadata` is `std::fs::metadata`, and
  `super::super::host::fs::read` is `std::fs::read`. Resolution is not by module, so it can only
  over-report; it is bounded at eight steps and 256 readings a path.
- **What cannot be followed is refused** (`hidden-path`, a Lumen rule): a glob import of a std
  module (`use std::*`, `use core::ops::*`, and through an alias, `use host::thread::*`), since
  it brings names into scope that the scan cannot list; a path of a macro body that starts at
  `std`, `core` or `alloc` (directly or through an alias) and has a metavariable segment
  (`std::$m::var_os`, `std::thread::$f`); and a path whose first segment is a metavariable other
  than `$crate` with a later segment that is a metavariable or a std module holding a banned item
  (`$root::fs::read`, `$a::$b::$c`). `<$t>::default()`, `$t::new()` and `$x + $x` pass.
- **`cfg` does not matter to the scan.** It is lexical: code under `cfg(windows)`,
  `cfg(target_arch = ...)` or inside `cfg_attr` is read like any other, and a
  `#[cfg(...)] mod x;` (with or without `#[path]`) is followed by the module walk whatever its
  `cfg`. Tests seed each form above under such a `cfg`.
- **Clippy on more targets is not worth a CI change.** It would add exact resolution of the
  core's paths under other targets' `cfg`s. The deterministic core has no code under a target
  `cfg` today (only tests do), and #155's amendment below makes one a finding, so every leg
  compiles the same core code the Linux run lints; the scan covers the rest. The cost would be
  a Clippy step on four more legs, and the fixture's seeded forms would need a per-target
  variant. Not done.
