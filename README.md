# Lumen

A deterministic ray tracer: a [Geordi](https://github.com/flyingrobots/geordi) runtime built on
[Bunny](https://github.com/flyingrobots/bunny)'s fixed-point math. The same scene renders to the same bytes on
every platform CI checks: x86-64 and aarch64 Linux, aarch64 macOS, x86-64 Windows and
WebAssembly (see [Determinism](#determinism)).

Work is tracked in GitHub Issues with `issue-dag/2` frontmatter.

## What lumen does today

See [how Lumen renders](docs/explanation.md).

## Render an image

See the [tutorial](docs/tutorial.md).

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

## Build and test

See [contributing](CONTRIBUTING.md).

## License

Apache-2.0. See [LICENSE](LICENSE).

The Stanford Bunny mesh in `assets/stanford-bunny/` is not covered by that license: it is from the
Stanford 3D Scanning Repository, Stanford Computer Graphics Laboratory, and is used with
attribution. See `assets/stanford-bunny/README.md`.
