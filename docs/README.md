# Lumen's documentation

Every page, by what you want to do. The [README](../README.md) is the short version.

## Start here

- [Render your first image](tutorial.md): build Lumen with Cargo, render two of its scenes, and see
  the image hash stay the same at another thread count.

## Look up

- [The `lumen` command](reference/cli.md): every command and option, the report line by line,
  the output file, exit statuses and every statistics key.
- [The `lumen` library](reference/library.md): the crate root by family, its rules and its
  errors; the rustdoc (`cargo doc --no-deps -p lumen --open`) is the full API reference.
- [The built-in scenes](reference/scenes.md): the three scenes object by object, with their tests.

## Understand

- [How Lumen renders, and why the image is the same everywhere](explanation.md): the
  integrator, the threads, the image hash and the determinism gates, ending in the
  [requirement-to-evidence map](explanation.md#requirements-and-their-evidence).

## Measure

- [Measure render time](how-to/measure-performance.md): take a timing that can be compared with
  the records.

## Contribute

- [Contributing to Lumen](../CONTRIBUTING.md): where the code lives, the promises a change can
  break, the full gate, edit paths by kind of change, and how a change reaches `main`.
- [The changelog](../CHANGELOG.md).

## Decision records

Each records a decision as it was taken; later changes are new records or dated clarification
sections, never rewrites.

| ADR | Decision | Status |
| --- | --- | --- |
| [0001](adr/0001-tile-scheduling.md) | Tile scheduling on worker threads | superseded by 0003 |
| [0002](adr/0002-library-api-surface.md) | The library's public API and the render's provenance | accepted; clarified 2026-11-14 |
| [0003](adr/0003-row-bands.md) | Row bands, results through `join`, whole-program release builds | accepted; clarified 2026-11-14 |
| [0004](adr/0004-webassembly-determinism-leg.md) | How WebAssembly is tested | accepted |
| [0005](adr/0005-scene-content-identity.md) | A scene's content identity | accepted; version 2 of the encoding; clarified 2026-11-14 |
| [0006](adr/0006-nondeterminism-gate.md) | How nondeterminism is kept out of the deterministic core | accepted |
| [0007](adr/0007-smooth-shading.md) | Smooth shading: the integrator's identity and the vertex normals | accepted; clarified 2026-11-14 |
| [0008](adr/0008-statistics.md) | Statistics: what is counted, where, and why it cannot change the image | accepted |
| [0009](adr/0009-image-hash.md) | The image hash | accepted (recorded 2026-11-14) |
| [0010](adr/0010-debug-views.md) | Debug views: selection, identity, provenance and stability | accepted |
| [0012](adr/0012-image-decoding.md) | Image decoding lives in a dependency-free `lumen-image` crate | accepted |

## Measurement records

Dated readings, each true of its commit only.

- [The 4K reference scene at 16 samples per pixel, 2026-11-13](measurements/2026-11-13-4k-reference.md)
- [The 4K reference scene with row bands and whole-program builds, 2026-11-13](measurements/2026-11-13-4k-row-bands.md)
- [The readings the README held, 2026-11-13](measurements/2026-11-13-readme-readings.md)
- [Fixed Point version 2 at 4K and 64 samples per pixel, 2026-11-14](measurements/2026-11-14-4k-fixed-point-v2.md)
- [The canyon scene at 4K and 16 samples per pixel, 2026-11-14](measurements/2026-11-14-canyon.md)
- [Traversal work per ray in the three built-in scenes, 2026-11-14](measurements/2026-11-14-traversal-cost.md)

## Coverage

What each part of Lumen has, by the documentation standard's page types. "Not needed" says why.

| Capability | Audience | Tutorial | How-to | Reference | Explanation | Troubleshooting | Contributor |
| --- | --- | --- | --- | --- | --- | --- | --- |
| The `lumen` command | users | [yes](tutorial.md) | [measure render time](how-to/measure-performance.md) | [yes](reference/cli.md) | [yes](explanation.md) | none yet: the exit statuses and messages are in the [reference](reference/cli.md#exit-status-and-errors) | [yes](../CONTRIBUTING.md) |
| The library | Rust developers | none yet: the crate root's doc example renders, hashes and encodes a scene | none yet | [map](reference/library.md) and rustdoc | [yes](explanation.md) | not needed: every failure is a typed error with its cause ([errors](reference/library.md#errors)) | [yes](../CONTRIBUTING.md) |
| The built-in scenes | users, scene authors | through the [tutorial](tutorial.md) | not needed: a scene is chosen by one option | [yes](reference/scenes.md) | in the [reference](reference/scenes.md) | not needed | [change a scene](../CONTRIBUTING.md#change-a-scene-or-add-one) |
| Determinism and its gates | maintainers | not needed | [record a measurement](../CONTRIBUTING.md#record-a-measurement) | [evidence map](explanation.md#requirements-and-their-evidence) | [yes](explanation.md#determinism) | none yet | [yes](../CONTRIBUTING.md#the-gates-in-ci) |
