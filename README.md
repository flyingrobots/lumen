# Lumen

A deterministic ray tracer: a [Geordi](https://github.com/flyingrobots/geordi) runtime built on
[Bunny](https://github.com/flyingrobots/bunny)'s fixed-point math. The same scene renders to the same bytes on
every platform CI checks: x86-64 and aarch64 Linux, aarch64 macOS, x86-64 Windows and
WebAssembly (see [Determinism](#determinism)).

Work is tracked in GitHub Issues with `issue-dag/2` frontmatter.

## What lumen does today

See [how Lumen renders](docs/explanation.md).

## Render an image

The short way, from a checkout:

```sh
make demo            # both built-in scenes at 1280x720, opened when done
make fixed-point     # just the Fixed Point scene
make 4k              # the reference scene at 3840x2160, 16 samples
make help            # every target and the variables you can override
```

Images land in `out/`. `make` runs the same container commands shown below, so it needs
`devrun` (set `DEVRUN=` if it is not at `~/git/devcontainers/bin/devrun`). Size, samples, seed
and the container's CPU limit are variables: `make fixed-point WIDTH=1920 HEIGHT=1080 CPUS=18`.

The long way:

Builds and runs happen in the shared containers through `devrun`, never on the host. Files
written under `/scratch/out/` in the container come back with `--export <name>` into a fresh
directory given by `--out`:

```sh
D=~/git/devcontainers/bin/devrun
python3 $D lumen ~/git/lumen --worktree --fetch --export reference.png --out "$(mktemp -d)" -- \
  bash -c 'mkdir -p /scratch/out && cargo run --locked --release -q -p lumen -- render --width 640 --height 360 --samples 64 --out /scratch/out/reference.png'
```

### Generated tests

A claim that quantifies over inputs (any split of a SHA-256 input, any image size, any order of a
pixel's samples) is tested over generated inputs from `crates/lumen/tests/generator/mod.rs`, a
seeded SplitMix64 (Lumen takes no property-testing crate). Each test fixes its seed, derives one
seed per case, prints the seed and the case seed in every failure message, keeps to a bounded
number of cases and states its oracle and run time in its comment. Replay one case with
`Gen::new(case_seed)`. A new generated test is shown to fail on a seeded mutation of the code it
checks before it merges. The tests are `tests/generated.rs`, `src/sha256.rs` and
`src/png/generated.rs`; the first also runs in the WebAssembly job, so none may assume a 64-bit
`usize`.

The command's report, its options and their values, its exit status and how `--out` publishes the
file are in the [command reference](docs/reference/cli.md).

## Library and command

The library's public surface, its errors and the command's edge are in the
[library reference](docs/reference/library.md).

## Statistics

`lumen stats` and `lumen render --stats`, with every key they print, are in the
[command reference](docs/reference/cli.md#statistics).

## The reference scene

The two built-in scenes, `lumen-reference-scene/3` and `lumen-fixed-point-scene/2`, triangle by
triangle, with their materials, lighting, cameras and tests, are in the
[scene reference](docs/reference/scenes.md).

## The Fixed Point scene

See the [scene reference](docs/reference/scenes.md#the-fixed-point-scene).

## The integrator

See [the integrator](docs/explanation.md#the-integrator).

## Performance

See [measure performance](docs/how-to/measure-performance.md).

## Determinism

See [determinism](docs/explanation.md#determinism).

## Bunny dependency

Lumen uses Bunny's `bunny-num`, `bunny-linalg`, `bunny-geom`, `bunny-query`, `bunny-broadphase`,
`bunny-optics` and `bunny-codec` (with default features off, so it needs no crates.io crate) at
version 0.7.1. The crates come in as git dependencies pinned to the full commit
`d4fe51d38fa10f8d6496bd110955314edac30bbf` of `https://github.com/flyingrobots/bunny`, a commit of Bunny's
main after the 0.7.1 release (`bunny-codec` also brings in `bunny-mesh`). They move to the crates.io registry when a release holds what lumen
needs. `BUNNY_REV` in `crates/lumen/src/lib.rs`
records the commit, and a test checks that `Cargo.lock` resolves every Bunny crate to it and lists no registry crate.

Dependencies build with `opt-level = 2` even in debug builds and tests, so the brute-force tests
and a default-size render take seconds; lumen's own code keeps the debug profile.

To move the pin, change the `rev` of every Bunny dependency in `Cargo.toml` and `BUNNY_REV`,
then regenerate `Cargo.lock` in the container and apply the exported patch:

```sh
python3 $D lumen ~/git/lumen --worktree --network --export-patch --out "$(mktemp -d)" -- \
  bash -c 'mkdir -p /tmp/ch && CARGO_HOME=/tmp/ch cargo update -w'
```

## Build and test

Builds and tests run in the shared containers through `devrun`, never on the host
(see `~/git/devcontainers/README.md`). The toolchain is pinned to 1.96.0 in
`rust-toolchain.toml`, the version of the container image.

```sh
D=~/git/devcontainers/bin/devrun
python3 $D lumen ~/git/lumen --worktree --fetch -- cargo fmt --all --check
python3 $D lumen ~/git/lumen --worktree -- cargo clippy --workspace --all-targets --locked -- -D warnings
python3 $D lumen ~/git/lumen --worktree -- cargo test --locked --workspace
python3 $D lumen ~/git/lumen --worktree -- bash -c 'RUSTDOCFLAGS="-D warnings" cargo doc --no-deps --locked'
python3 $D lumen ~/git/lumen --worktree -- cargo check -p lumen --lib --target wasm32-unknown-unknown --locked
```

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
    --test fixed_point --test generated
```

### Generated tests

A claim that quantifies over inputs (any split of a SHA-256 input, any image size, any order of a
pixel's samples) is tested over generated inputs from `crates/lumen/tests/generator/mod.rs`, a
seeded SplitMix64 (Lumen takes no property-testing crate). Each test fixes its seed, derives one
seed per case, prints the seed and the case seed in every failure message, keeps to a bounded
number of cases and states its oracle and run time in its comment. Replay one case with
`Gen::new(case_seed)`. A new generated test is shown to fail on a seeded mutation of the code it
checks before it merges. The tests are `tests/generated.rs`, `src/sha256.rs` and
`src/png/generated.rs`; the first also runs in the WebAssembly job, so none may assume a 64-bit
`usize`.

`--fetch` is needed on the first run in a worktree; it downloads the Bunny git dependency, and
later phases build offline. Use `--commit HEAD` instead of `--worktree` for evidence pinned to a
commit.

Lint posture mirrors Bunny: `unsafe` is forbidden, clippy `all` is denied and
`pedantic` warns, and CI runs with warnings denied.

### Supply-chain, size and coverage checks

`.github/workflows/gates.yml` runs six jobs on every pull request:

| Job | What it checks | Fails when |
|---|---|---|
| `Gates / cargo-deny` | `cargo deny --locked check` against `deny.toml` | an advisory, a yanked crate, a licence other than Apache-2.0, a second version of a crate, a wildcard version, any crate from a registry (crates.io included), a git repository other than `https://github.com/flyingrobots/bunny`, or a git dependency not pinned by `rev` |
| `Gates / cargo-audit` | `cargo audit --deny warnings` on `Cargo.lock` | a RustSec advisory or warning for a locked crate |
| `Gates / file length` | `cargo run -p xtask -- file-length` | a source file over 300 lines of code without a listed exception, or over 500 at all; a test file (one compiled only under `#[cfg(test)]`, or an integration test) over 750; a listed file grown past its exception; an exception no longer needed; an `include!`, or a `#[path]` to anything but a `.rs` file the gate reads |
| `Gates / nondeterminism` | `cargo run -p xtask -- nondeterminism` (see "Nondeterminism bans") | a float, a banned name or `std` path in the deterministic core, a malformed or unused scan waiver, a ban lint waived with `allow` or without rule IDs in its reason, or the `include!` and `#[path]` forms above |
| `Gates / doc links` | `cargo run -p xtask -- doc-links` | a relative link in a Markdown file to a file or directory that does not exist or out of the repository, or a `#anchor` that is no heading of its page (GitHub's anchors); links with a scheme are not followed |
| `Signal / coverage` | `cargo llvm-cov --workspace --summary-only`, in the job summary | never: coverage is reported, not gated |

A line of code is a line with a character outside a comment; doc comments are comments, and so
is a shebang line, which both gates skip exactly as `rustc` does (ADR 0006). The
exceptions live in `xtask/src/file_length/exceptions.rs`, each with the count it was granted at,
the date and the issue that splits the file.

Locally, in the container (it has `cargo-deny`; the advisory database needs the network and a
writable `CARGO_HOME`):

```sh
python3 $D lumen ~/git/lumen --worktree -- cargo run --locked -q -p xtask -- file-length
python3 $D lumen ~/git/lumen --worktree -- cargo run --locked -q -p xtask -- doc-links
python3 $D lumen ~/git/lumen --worktree --network -- \
  bash -c 'mkdir -p /tmp/ch && CARGO_HOME=/tmp/ch cargo deny --locked check'
```

The container has neither `cargo-audit` nor `cargo-llvm-cov`; CI installs both as pinned
prebuilt binaries. To run them in the container, install them under `/tmp` first, for example
`cargo install --locked cargo-audit --version 0.22.2 --root /tmp/audit` with `--network`.

## Contributing

Run the gates under "Build and test" before opening a pull request, and add a line to
`CHANGELOG.md` under `Unreleased`. A nondeterminism waiver (see "Nondeterminism bans") is asked
for in the pull request: its reason says why the banned item cannot reach a pixel, a hash or a
digest, and review accepts or refuses it there.

### Tests

- **Size classes** (testing standard rule 9). Every integration test file says its class in the
  first line of its module comment, `//! Size class: small|medium`. Unit tests in `src/` are small.
  *Small*: in one process, no file, no network, no clock. The renderer's own worker threads are
  allowed because they are the code under test. *Medium*: spawns the `lumen` process, writes
  files, or runs real threads at fixed counts. Run one class by file: `cargo test --test cli
  --test threads --test counts` is the medium set; `cargo test --lib --test render --test library --test
  fixed_point --test statistics` the small set. There is no large class yet.
- **Hermetic files** (rule 8). A test that writes files makes a `Scratch` directory (see
  `crates/lumen/tests/cli.rs`) named for the test and the process id under `CARGO_TARGET_TMPDIR`,
  runs the command with it as the working directory, passes absolute paths and removes it when
  done, so the test and release profiles can run at once.
- **One behaviour per test**, named as a sentence about it, with no platform branch inside the
  body: a platform difference is a `#[cfg]` on its own test.

## License

Apache-2.0. See [LICENSE](LICENSE).

The Stanford Bunny mesh in `assets/stanford-bunny/` is not covered by that license: it is from the
Stanford 3D Scanning Repository, Stanford Computer Graphics Laboratory, and is used with
attribution. See `assets/stanford-bunny/README.md`.
