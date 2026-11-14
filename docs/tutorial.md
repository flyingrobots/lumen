# Render your first image

You will build Lumen from source, render its reference scene to a PNG file, and see the property
Lumen exists for: the image is the same bytes however it is computed. It takes a Rust toolchain
and a network connection for the first build; nothing else.

## Before you begin

- **Rust 1.96.0**, through [rustup](https://rustup.rs/). The repository's `rust-toolchain.toml`
  pins it, and rustup installs it on the first `cargo` command in the checkout; if yours does
  not, run `rustup toolchain install 1.96.0` there. Without rustup, any Rust 1.96 or later
  builds it (`rust-version` in `Cargo.toml`), but 1.96.0 is the toolchain CI tests.
- **Git and a network connection** for the first build: Cargo fetches Lumen's only dependency,
  Bunny's crates, from `https://github.com/flyingrobots/bunny` at the commit `Cargo.lock` pins.
  Lumen uses no crates.io crate.
- About 7 MB of disk for the checkout, and BUILD_SIZE for the build in `target/`.

## 1. Get the source

```sh
git clone https://github.com/flyingrobots/lumen
cd lumen
```

## 2. Render the reference scene

```sh
cargo run --release -p lumen -- render --out first.png
```

The first run compiles Bunny and Lumen with whole-program optimization, which takes a while;
later runs start at once. Cargo's own lines (`Compiling ...`, `Finished ...`, `Running ...`) go
to standard error. Then `lumen` prints its report. Abridged: the lines shown are exact, and the
timing lines it also prints vary from run to run:

```text
scene lumen-reference-scene/3
integrator lumen-diffuse-path/1
size 320x180
samples 4
seed 0
image sha256:77ab18fafd84f6598fa638281e205f2298df6ba6da3471546a3724718d3cef23
png first.png (230699 bytes)
```

`first.png` is a 320 by 180 picture: a red bumpy sphere, a blue octahedron and a green Stanford
bunny on a pale floor before a stone wall, lit low from the right, with long shadows. Open it
with any image viewer.

The `image` line is the image hash: the SHA-256 of the picture's raw pixel bytes. If yours
differs from the line above, something is wrong: Lumen promises this exact value on every
machine CI checks, and a test (`tests/tutorial.rs`) runs these steps there.

## 3. Render it again on one thread

```sh
cargo run --release -p lumen -- render --threads 1
```

Without `--out` nothing is written (the report says `png none`), but the hash is still printed:

```text
threads 1
image sha256:77ab18fafd84f6598fa638281e205f2298df6ba6da3471546a3724718d3cef23
```

It is the same hash. The first render used every core; this one used one. Every number that
decides a pixel is an integer computed by Bunny, and each pixel is computed by one worker on its
own, so neither the thread count nor the order the work happens in can change a byte.

## 4. Try the other scene

```sh
cargo run --release -p lumen -- render --scene fixed-point --out fixed-point.png
```

```text
scene lumen-fixed-point-scene/2
image sha256:857f44a50529d6f2656dd7b4279ec6b3aeb5a4e9c6a5c11575896de2e339a95b
png fixed-point.png (230699 bytes)
```

Fixed Point is the Stanford bunny in sunglasses at a craps table, both dice showing four pips on
every face. For a larger picture, add `--width 1280 --height 720 --samples 64`: more pixels and
more samples per pixel take longer and give a different hash, the same one on every machine.

## With the container runner

If you have the maintainers' container runner, `devrun`, at `~/git/devcontainers/bin/devrun`,
`make demo` renders both scenes at 1280 by 720 in the container, puts them in `out/` and opens
them; `make help` lists every target. It is a shortcut for maintainers, not needed for anything
above.

## What you learned

- `lumen render` builds a scene, renders it, prints a report and, with `--out`, writes a PNG.
- The `image` line names the picture exactly, and it does not depend on the thread count.
- A scene is chosen with `--scene`; size and samples change the picture and its hash.

## Next steps

- Every command, option and report line: the [command reference](reference/cli.md).
- How the renderer works and why the hash is stable: [how Lumen renders](explanation.md).
- The two scenes in detail: the [scene reference](reference/scenes.md).
- Using the library from Rust: the [library reference](reference/library.md).
