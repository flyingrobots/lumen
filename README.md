# Lumen

Lumen is a deterministic ray tracer: a [Geordi](https://github.com/flyingrobots/geordi) runtime
built on [Bunny](https://github.com/flyingrobots/bunny)'s fixed-point math. It renders a scene to
the same bytes, and so the same image hash, on every run, at every thread count, and on every
platform CI checks: x86-64 and aarch64 Linux, aarch64 macOS, x86-64 Windows and WebAssembly
(`wasm32-wasip1`). Each part of that claim is mapped to the tests that check it in the
[evidence map](docs/explanation.md#requirements-and-their-evidence).

It renders three built-in scenes: a reference scene (a bumpy red sphere, a blue octahedron and
the Stanford bunny before a stone wall, under a low sun and a pale sky), Fixed Point (the
full-resolution bunny in sunglasses at a craps table) and a canyon (three craft flying along a
fractal canyon under a banded planet, 566 005 triangles). Every sample is one diffuse path with a
shadow ray and one bounce, all in Bunny's integer arithmetic, and the image is written as a PNG.

## Render an image

With a Rust toolchain ([rustup](https://rustup.rs/); the checkout pins Rust 1.96.0):

```sh
git clone https://github.com/flyingrobots/lumen
cd lumen
cargo run --release -p lumen -- render --out first.png
```

The report ends with the image hash, which is the same on every platform CI checks:

```text
image sha256:77ab18fafd84f6598fa638281e205f2298df6ba6da3471546a3724718d3cef23
png first.png (230699 bytes)
```

The [tutorial](docs/tutorial.md) goes on from there.

## Documentation

- Learn: [render your first image](docs/tutorial.md).
- Look up: [the `lumen` command](docs/reference/cli.md), [the library](docs/reference/library.md),
  [the built-in scenes](docs/reference/scenes.md).
- Understand: [how Lumen renders, and why the image is the same everywhere](docs/explanation.md).
- Measure: [measure render time](docs/how-to/measure-performance.md).
- Decisions and records: the [documentation index](docs/README.md) lists every ADR and
  measurement record.
- Contribute: [contributing](CONTRIBUTING.md) and the [changelog](CHANGELOG.md). Work is tracked
  in GitHub Issues with `issue-dag/2` frontmatter.

## License

Apache-2.0. See [LICENSE](LICENSE).

The Stanford Bunny mesh in `assets/stanford-bunny/` is not covered by that license: it is from the
Stanford 3D Scanning Repository, Stanford Computer Graphics Laboratory, and is used with
attribution. See `assets/stanford-bunny/README.md`.
