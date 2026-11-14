# Stanford Bunny

**Assets**:

- `bun_zipper_res3.ply`, the decimated Stanford bunny (3 851 faces), used in lumen's reference
  scene (`lumen-reference-scene/4`, and version 3 before it) and in Fixed Point version 1 (see
  the repository README);
- `bun_zipper.ply`, the full-resolution Stanford bunny (69 451 faces), for Fixed Point version 2
  (flyingrobots/lumen#39).

Both are Stanford's data under the same terms, below.

## Source and attribution

The mesh is the Stanford Bunny from the **Stanford 3D Scanning Repository, Stanford Computer
Graphics Laboratory**. It is Stanford's data, not lumen's: lumen's Apache-2.0 license does not
apply to it, and it is used here with attribution as Geordi uses it. Stanford asks users of the
repository to acknowledge the source, allows research use and free redistribution, and requires
permission for commercial product use. Keep this attribution with the file wherever it is used.

- Source archive: `https://graphics.stanford.edu/pub/3Dscanrep/bunny.tar.gz`
  (archive SHA-256 `a5720bd96d158df403d153381b8411a727a1d73cff2f33dc9b212d6f75455b84`, as
  recorded by Geordi, retrieved 2026-05-23)
- Path in the archive: `bunny/reconstruction/bun_zipper_res3.ply`
- Lumen's copy is taken byte for byte from Geordi's checked-in asset
  (`fixtures/render-everywhere/assets/stanford-bunny/` in `flyingrobots/geordi`) and has the same
  SHA-256:
  - `bun_zipper_res3.ply`: `975e7f9b160b4ea15b0e225e21b10828ebcf678df020d2f6a46aa408fdcf5cd6`
  - `STANFORD_RECONSTRUCTION_README.txt`:
    `b05569908cfe8f50cbd2babeec2c5453a2ae6e7ed739f238fdbd00a1854a06de`
- As Geordi records, the committed PLY and the reconstruction README have had line-end
  whitespace normalized for repository hygiene, so their bytes differ from the archive's in
  trailing whitespace only.

Stanford's own description of the reconstructions is preserved beside the file as
`STANFORD_RECONSTRUCTION_README.txt`. It notes that the decimated (`_res*`) meshes were made with
a crude algorithm that does not necessarily preserve mesh topology.

The repository's `.gitattributes` marks `assets/**` as `-text`, so Git never rewrites these
files' line endings, and a test
(`reference::bunny::tests::the_committed_asset_has_geordis_hash`) checks the PLY's SHA-256 with
lumen's own `sha256` module.

## Mesh profile

- ASCII PLY 1.0, one `comment zipper output` line
- 1 889 vertices with properties `x y z confidence intensity` (lumen uses `x y z` only)
- 3 851 triangular faces (`property list uchar int vertex_indices`)
- Bounds min `[-0.0943643, 0.0334143, -0.0616721]`, max `[0.0609346, 0.184813, 0.0584651]`
  (as in Geordi's `bunny.mesh.json`)
- One coordinate in exponent form (`8.20736e-05`); the others are plain decimals

Lumen reads it with Bunny's ASCII PLY reader (`bunny_codec::parse_ascii_ply`, profile
`bunny-ascii-ply-triangle-mesh/1`, decimals converted by `bunny-decimal-q32-32/1`), through
`crates/lumen/src/mesh/loaded.rs`.

## The full-resolution mesh, `bun_zipper.ply`

- Source archive: `https://graphics.stanford.edu/pub/3Dscanrep/bunny.tar.gz`, retrieved
  2026-11-14 directly from Stanford (4 894 286 bytes). Its SHA-256 is
  `a5720bd96d158df403d153381b8411a727a1d73cff2f33dc9b212d6f75455b84`, the value Geordi recorded,
  checked before extracting.
- Path in the archive: `bunny/reconstruction/bun_zipper.ply`. Only that file was extracted.
- Lumen's copy is the archive's file **byte for byte, with no normalization**: 3 033 195 bytes,
  SHA-256 `b1acc63bece78444aa2e15bdcc72371a201279b98c6f5d4b74c993d02f0566fe`. Unlike the
  decimated copy above, its line-end whitespace is kept: every data line ends in a space before
  its line feed (105 398 lines), which Bunny's reader trims. Line endings are LF; the file has no
  carriage return. `assets/** -text` in `.gitattributes` keeps Git from changing them.
- Stanford's reconstruction README (`STANFORD_RECONSTRUCTION_README.txt`, above) describes this
  file too: it is the zippered reconstruction that the `_res*` meshes were decimated from.

A test (`scenes::fixed_point::full_bunny::tests`) checks the SHA-256 and the length with lumen's
own `sha256` module, and the counts, bounds and topology below as Bunny's reader reads it.

### Mesh profile

- ASCII PLY 1.0, one `comment zipper output` line, the same header layout as the decimated file
- 35 947 vertices with properties `x y z confidence intensity` (lumen uses `x y z` only) and
  69 451 triangular faces, as Stanford describes it
- Bounds min `[-0.0946899, 0.0329874, -0.0618736]`, max `[0.0610091, 0.187321, 0.0587997]`
  (the file's own extreme decimals; vertices 12 284, 33 259, 23 959 and 12 676, 23 637, 3 284)
- 78 coordinates in exponent form (such as `e-05`); the others are plain decimals

### Topology, measured

Counted twice, by an independent script on the file's text with exact decimals and by the test
on Bunny's reading of it, with the same results:

| Property | Count |
| --- | ---: |
| Vertices at a position another vertex also has | 0 |
| Vertices that no face names | 1 113 |
| Faces that repeat an index, or another face's three vertices | 0 |
| Edges | 104 288 |
| Boundary edges (one face) | 223, in five loops of 80, 42, 40, 39 and 22: the holes in the base |
| Edges of three or more faces | 0 |
| Shared edges traversed the same way by both faces | 0 |

So, unlike the decimated mesh, the full mesh is manifold and consistently wound, with no
duplicate faces; its only defects are the five holes in its base, where the scanner could not
see. Zero-area faces and coincident vertices can still appear once a scene scales and floors it
to the `2^-16` grid; the scene that places it counts them.

### Size

The file is embedded with `include_str!` in the library, in
`crates/lumen/src/scenes/fixed_point/full_bunny.rs`, as the decimated one is: assets are handled
as before, and the library still reads no file. Measured in the release profile (rust-dev:1.96.0
container, Apple M5 Pro, flyingrobots/lumen#39):

| What | Without the asset | With it |
| --- | ---: | ---: |
| `lumen` command, the asset unused | 812 688 bytes | 812 688 bytes |
| `lumen` command, in an experimental build whose Fixed Point scene reads it (no scene does yet) | 812 688 bytes | 3 892 904 bytes |
| A program that renders only the reference scene (`examples/traversal.rs`) | 724 872 bytes | 724 872 bytes |
| `liblumen` rlib for `wasm32-unknown-unknown` (an intermediate file) | 1 648 722 bytes | 7 826 206 bytes |
| Rebuilding the `lumen` crate and command after a touch, three runs | 2.48 to 2.62 s | 2.50 to 2.80 s |

So the linker keeps the bytes only in a program that reaches them (the command, once a
scene is built from them): about 3.08 MB there and nothing elsewhere. The compile time does not change measurably. A library that reads the file at run
time would need file access, which the library does not have (ADR 0002), and a WebAssembly
host to provide it; embedding only for one scene is what the linker already does. The file
compresses to about 1.05 MB (gzip -9), so it roughly doubles the size of a clone (the
repository's pack was 876 KiB before it). The crate is not published (`publish = false`), so
there is no package size to keep down.
