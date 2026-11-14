# Contributing to Lumen

This guide is for changing Lumen: where things live, which promises a change can break, how to
verify it and how it reaches `main`. To use Lumen, start with the [tutorial](docs/tutorial.md);
for how it works, [how Lumen renders](docs/explanation.md).

## What you are changing

Lumen is two targets in one crate, `crates/lumen`. The library renders: it takes a scene, a size,
settings and a thread count, and returns an image whose bytes depend only on the first three. The
`lumen` command (`src/bin/lumen/`) is a front end: it parses arguments, reads the clock, asks the
host for a thread count, writes files and prints. Everything outside the library's arguments
stays in the command ([library reference](docs/reference/library.md#the-library-touches-nothing-outside-its-arguments)).
`xtask` holds the repository's own checks.

| Concern | Where | Notes |
| --- | --- | --- |
| Scene model, the ray-query port, the tree adapter, content identity, statistics | `crates/lumen/src/scene/` | ADR 0005, ADR 0008 |
| The integrator and the render entry points | `crates/lumen/src/render.rs`, `render/` | ADR 0007 |
| The scheduler: row bands on threads | `crates/lumen/src/workers.rs`, `workers/`, `schedule.rs` | ADR 0003; simulated in `workers/tests.rs` |
| Images, the image hash, SHA-256, PNG | `crates/lumen/src/image.rs`, `sha256.rs`, `png.rs` | ADR 0009 |
| Provenance | `crates/lumen/src/provenance.rs` | ADR 0002 |
| The built-in scenes' content | `crates/lumen/src/reference/`, `scenes/`, `mesh/`, `builtin_scene.rs`; the meshes in `assets/` | [scene reference](docs/reference/scenes.md) |
| The command | `crates/lumen/src/bin/lumen/` | [command reference](docs/reference/cli.md) |
| Golden values | `crates/lumen/tests/golden/mod.rs` | below |
| Gates | `xtask/`, `clippy.toml`, `deny.toml`, `.github/workflows/` | ADR 0006 |

## Promises a change can break

Each is a row of the [requirement-to-evidence map](docs/explanation.md#requirements-and-their-evidence),
which names the tests that hold it. A change that weakens one changes that row in the same pull
request.

- **The image is a function of the scene, the size and the settings** (R-DET-1 to R-DET-5): no
  thread count, band height, platform, profile or order may change a byte. Nothing from the host
  may reach the library (R-NDET-1).
- **Equal provenance means equal bytes** for a scene rendered as its own tracer (R-PROV-1,
  R-PROV-2). Anything that can change a pixel is an identity in the provenance: a scene version,
  an integrator, shading, sampler, tone or encoding identity, or the Bunny commit.
- **The command's output and exit codes are a contract**: report lines are only ever added, and
  statistics keys never renamed.
- **No third-party crate** (R-DEP-1): Bunny's crates from one git repository, pinned by commit.

## Make a change

Builds, tests and benchmarks run in the shared containers through `devrun`, never on the host
(see `~/git/devcontainers/README.md`); a contributor without it can still build and test with
plain `cargo`, as the [tutorial](docs/tutorial.md) does, but the commands below are the ones the
maintainers' records use. The toolchain is pinned to 1.96.0 in `rust-toolchain.toml`, the
version of the container image. `--fetch` is needed on the first run in a worktree; it downloads
the Bunny git dependency, and later phases build offline. Use `--commit HEAD` instead of
`--worktree` for evidence pinned to a commit.

### The full gate

Run all of it before opening a pull request; CI runs the same:

```sh
D=~/git/devcontainers/bin/devrun
python3 $D lumen ~/git/lumen --worktree --fetch -- cargo fmt --all --check
python3 $D lumen ~/git/lumen --worktree -- cargo clippy --workspace --all-targets --locked -- -D warnings
python3 $D lumen ~/git/lumen --worktree -- cargo test --locked --workspace
python3 $D lumen ~/git/lumen --worktree -- cargo test --locked --workspace --release
python3 $D lumen ~/git/lumen --worktree -- bash -c 'RUSTDOCFLAGS="-D warnings" cargo doc --no-deps --locked'
python3 $D lumen ~/git/lumen --worktree -- cargo check -p lumen --lib --target wasm32-unknown-unknown --locked
python3 $D lumen ~/git/lumen --worktree -- cargo run --locked -q -p xtask -- nondeterminism
python3 $D lumen ~/git/lumen --worktree -- cargo run --locked -q -p xtask -- clippy-bans
python3 $D lumen ~/git/lumen --worktree -- cargo run --locked -q -p xtask -- file-length
python3 $D lumen ~/git/lumen --worktree -- cargo run --locked -q -p xtask -- doc-links
python3 $D lumen ~/git/lumen --worktree --network -- \
  bash -c 'mkdir -p /tmp/ch && CARGO_HOME=/tmp/ch cargo deny --locked check'
```

`cargo fmt` cannot be applied on the host; run `cargo fmt --all` in the container with
`--export-patch --out "$(mktemp -d)"` and `git apply` the patch it writes. `make test` runs the
tests in both profiles; `make nondeterminism`, `make clippy-bans` and `make doc-links` run those
checks.

The tests include the doc examples (one per workflow: build a scene, render it, hash it and
record its provenance, write a PNG into memory) and `crates/lumen/tests/library.rs`, which renders
through the library alone. The `wasm32-unknown-unknown` check shows the library builds without the
command.

The WebAssembly tests need the `wasm32-wasip1` target and Node 20 or later, which the container
image does not have; CI runs them (`Determinism / WebAssembly (wasm32-wasip1, Node)`). Where both
are installed, from the repository root:

```sh
rustup target add wasm32-wasip1
CARGO_TARGET_WASM32_WASIP1_RUNNER="node $PWD/scripts/wasi-run.mjs" \
  cargo test --locked -p lumen --target wasm32-wasip1 --lib --test render --test library \
    --test fixed_point --test generated --test smooth
```

Lint posture mirrors Bunny: `unsafe` is forbidden, clippy `all` is denied and
`pedantic` warns, and CI runs with warnings denied.

### Change a scene, or add one

A built-in scene's triangles, their order, its materials, normals, lighting and camera are its
version: any change is a new version (`lumen-reference-scene/4`), which changes its golden image
hash and its content digest. Keep the old values in the constants' history. Document the scene
in the [scene reference](docs/reference/scenes.md), and give a new one the tests the existing
ones have (counts, the trace domain, rays against brute force, a golden hash).

### Change a golden value

The golden image hashes and content digests (`crates/lumen/tests/golden/mod.rs`) are one value
for every platform and profile. Updating one is legitimate only when an identity in the
provenance, the settings of the golden render or the Bunny commit changes, and the new value is
the intended effect of that change. Any other change of a golden value is a determinism bug: do
not update it to make a test pass. Record each update in the constant's comment (the old value,
what changed, the issue), never add a per-platform value, and let every Determinism job confirm
the new value.

### Move the Bunny pin

Lumen uses Bunny's `bunny-num`, `bunny-linalg`, `bunny-geom`, `bunny-query`, `bunny-broadphase`,
`bunny-optics` and `bunny-codec` (with default features off, so it needs no crates.io crate) at
version 0.7.1. The crates come in as git dependencies pinned to the full commit
`2cd551e248d55407a8f27e84dcb26d1b1d8768bd` of `https://github.com/flyingrobots/bunny`, a commit of Bunny's
main after the 0.7.1 release (`bunny-codec` also brings in `bunny-mesh`). They move to the crates.io registry when a release holds what lumen
needs. `BUNNY_REV` in `crates/lumen/src/lib.rs`
records the commit, and a test checks that `Cargo.lock` resolves every Bunny crate to it and lists no registry crate.

Dependencies build with `opt-level = 2` even in debug builds and tests, so the brute-force tests
and a default-size render take seconds; lumen's own code keeps the debug profile.

To move the pin, change the `rev` of every Bunny dependency in `Cargo.toml` and `BUNNY_REV`,
then regenerate `Cargo.lock` in the container and apply the exported patch (`git apply`):

```sh
python3 ~/git/devcontainers/bin/devrun lumen ~/git/lumen --worktree --network --export-patch --out "$(mktemp -d)" -- \
  bash -c 'mkdir -p /tmp/ch && CARGO_HOME=/tmp/ch cargo update -w'
```

Then run the whole test suite: the golden hashes say whether the new Bunny changes a pixel. If
one changes, the change is legitimate only when it is the intended effect of the new Bunny
(below, "Change a golden value"); record the Bunny commit in the constant's history.

### Change the command

Update the [command reference](docs/reference/cli.md) with it: tests fail if the page does not
quote the `--help` text exactly, lacks a row for an option, or lacks a statistics key the command
prints. A report line or statistics key is added at the end of its group, never renamed. If a
change alters a line the [tutorial](docs/tutorial.md) shows, `tests/tutorial.rs` fails until the
tutorial says what the program prints.

### Add a test

#### Generated tests

A claim that quantifies over inputs (any split of a SHA-256 input, any image size, any order of a
pixel's samples) is tested over generated inputs from `crates/lumen/tests/generator/mod.rs`, a
seeded SplitMix64 (Lumen takes no property-testing crate). Each test fixes its seed, derives one
seed per case, prints the seed and the case seed in every failure message, keeps to a bounded
number of cases and states its oracle and run time in its comment. Replay one case with
`Gen::new(case_seed)`. A new generated test is shown to fail on a seeded mutation of the code it
checks before it merges. The tests are `tests/generated.rs`, `src/sha256.rs` and
`src/png/generated.rs`; the first also runs in the WebAssembly job, so none may assume a 64-bit
`usize`.

#### How tests are written

- **Size classes** (testing standard rule 9). Every integration test file says its class in the
  first line of its module comment, `//! Size class: small|medium`. Unit tests in `src/` are small.
  *Small*: in one process, no file, no network, no clock. The renderer's own worker threads are
  allowed because they are the code under test. *Medium*: spawns the `lumen` process, writes
  files, or runs real threads at fixed counts. Run one class by file: `cargo test --test cli
  --test threads --test counts --test tutorial` is the medium set; `cargo test --lib --test render --test library --test
  fixed_point --test statistics` the small set. There is no large class yet.
- **Hermetic files** (rule 8). A test that writes files makes a `Scratch` directory (see
  `crates/lumen/tests/cli.rs`) named for the test and the process id under `CARGO_TARGET_TMPDIR`,
  runs the command with it as the working directory, passes absolute paths and removes it when
  done, so the test and release profiles can run at once.
- **One behaviour per test**, named as a sentence about it, with no platform branch inside the
  body: a platform difference is a `#[cfg]` on its own test.

### Record a decision

A decision about content identity, a format, concurrency, the public API's compatibility or the
determinism gates gets an ADR in `docs/adr/`, numbered after the last one: context, decision,
consequences and what was rejected, with its status and issue on the first line. An ADR is a
record. A later change of mind is a new ADR that supersedes it (as ADR 0003 superseded ADR
0001), or a dated "Clarification" section that says what changed and why, flagged in its status
line; the original text stays.
The list is in the [documentation index](docs/README.md).

### Record a measurement

A timing worth keeping is a dated record in `docs/measurements/`, taken alone on the machine
([measure render time](docs/how-to/measure-performance.md)). Timings are never copied into other
pages; link to the record.

## The gates in CI

`.github/workflows/ci.yml` runs `fmt, clippy` (with the `clippy.toml` bans and `clippy-bans`) and
the five `Determinism / ...` jobs (each platform's whole test suite in both profiles, and the
WebAssembly job above).

### The supply-chain, size, link and coverage jobs

`.github/workflows/gates.yml` runs six jobs on every pull request:

| Job | What it checks | Fails when |
|---|---|---|
| `Gates / cargo-deny` | `cargo deny --locked check` against `deny.toml` | an advisory, a yanked crate, a licence other than Apache-2.0, a second version of a crate, a wildcard version, any crate from a registry (crates.io included), a git repository other than `https://github.com/flyingrobots/bunny`, or a git dependency not pinned by `rev` |
| `Gates / cargo-audit` | `cargo audit --deny warnings` on `Cargo.lock` | a RustSec advisory or warning for a locked crate |
| `Gates / file length` | `cargo run -p xtask -- file-length` | a source file over 300 lines of code without a listed exception, or over 500 at all; a test file (one compiled only under `#[cfg(test)]`, or an integration test) over 750; a listed file grown past its exception; an exception no longer needed; an `include!`, or a `#[path]` to anything but a `.rs` file the gate reads |
| `Gates / nondeterminism` | `cargo run -p xtask -- nondeterminism` (see [the bans](docs/explanation.md#nondeterminism-bans)) | a float, a banned name or `std` path in the deterministic core, a malformed or unused scan waiver, a ban lint waived with `allow` or without rule IDs in its reason, or the `include!` and `#[path]` forms above |
| `Gates / doc links` | `cargo run -p xtask -- doc-links` | a relative link in a Markdown file to a file or directory that does not exist or out of the repository, or a `#anchor` that is no heading of its page (GitHub's anchors); links with a scheme are not followed |
| `Signal / coverage` | `cargo llvm-cov --workspace --summary-only`, in the job summary | never: coverage is reported, not gated |

A line of code is a line with a character outside a comment; doc comments are comments, and so
is a shebang line, which both gates skip exactly as `rustc` does (ADR 0006). The
exceptions live in `xtask/src/file_length/exceptions.rs`, each with the count it was granted at,
the date and the issue that splits the file.

Locally, in the container (it has `cargo-deny`; the advisory database needs the network and a
writable `CARGO_HOME`), with `D=~/git/devcontainers/bin/devrun`:

```sh
python3 $D lumen ~/git/lumen --worktree -- cargo run --locked -q -p xtask -- file-length
python3 $D lumen ~/git/lumen --worktree -- cargo run --locked -q -p xtask -- doc-links
python3 $D lumen ~/git/lumen --worktree --network -- \
  bash -c 'mkdir -p /tmp/ch && CARGO_HOME=/tmp/ch cargo deny --locked check'
```

The container has neither `cargo-audit` nor `cargo-llvm-cov`; CI installs both as pinned
prebuilt binaries. To run them in the container, install them under `/tmp` first, for example
`cargo install --locked cargo-audit --version 0.22.2 --root /tmp/audit` with `--network`.

A nondeterminism waiver ([the bans](docs/explanation.md#nondeterminism-bans)) is asked for in
the pull request: its reason says why the banned item cannot reach a pixel, a hash or a digest,
and review accepts or refuses it there.

## How a change reaches `main`

`main` is protected by a repository ruleset. A change reaches it only through a pull request
that:

- has every commit signed;
- has all its review threads resolved;
- is up to date with `main` (the project merges `main` into the branch rather than rebasing a
  branch under review);
- passes the ten required checks: `fmt, clippy`, `Determinism / Linux x86-64`,
  `Determinism / Linux aarch64`, `Determinism / macOS aarch64`, `Determinism / Windows x86-64`,
  `Determinism / WebAssembly (wasm32-wasip1, Node)`, `Gates / cargo-deny`, `Gates / cargo-audit`,
  `Gates / file length` and `Gates / nondeterminism`;
- is merged with a merge commit (the only method allowed).

`main` cannot be deleted or force-pushed. `Gates / doc links` and `Signal / coverage` run on
every pull request but are not required.

Each pull request adds a line to `CHANGELOG.md` under `Unreleased` and updates the documentation
of whatever it changes: the reference page, the explanation, the evidence map, an ADR. Commit
messages say what changed and why, and end with an `Issue: flyingrobots/lumen#N` trailer.
