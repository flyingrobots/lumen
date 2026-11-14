# Lumen

A deterministic ray tracer: a [Geordi](https://github.com/flyingrobots/geordi) runtime built on
[Bunny](https://github.com/flyingrobots/bunny)'s fixed-point math. The same scene renders to the same bytes on
every machine.

Work is tracked in GitHub Issues with `issue-dag/2` frontmatter.

## What lumen does today

Lumen renders one built-in reference scene to a PNG file:

- one thread;
- one camera ray per pixel, through the pixel centre;
- the nearest triangle from Bunny's `closest_hit` over a BVH built with `build_bvh`;
- shading by the triangle's normal (no lights), and a constant sky colour for rays that hit
  nothing;
- Bunny's pixel path: `PixelAccumulator`, the `bunny-tone-srgb/1` tone curve, `write_rgba8`.

It does not yet compute lighting, shadows or bounces (flyingrobots/lumen#3), take several samples
per pixel (also #3), use more than one thread (#4), or read scenes from files (#6).

## Render an image

Builds and runs happen in the shared containers through `devrun`, never on the host. Files
written under `/scratch/out/` in the container come back with `--export <name>` into a fresh
directory given by `--out`:

```sh
D=~/git/devcontainers/bin/devrun
python3 $D lumen ~/git/lumen --worktree --fetch --export reference.png --out "$(mktemp -d)" -- \
  bash -c 'mkdir -p /scratch/out && cargo run --locked --release -q -p lumen -- render --width 640 --height 360 --out /scratch/out/reference.png'
```

`--fetch` is needed on the first run in a worktree. The command prints:

```text
scene lumen-reference-scene/1
shading lumen-normal-shading/1
tone bunny-tone-srgb/1
bunny 7fdd2e4926041b0a226567e0a6e1ec51e522304f
size 640x360
image sha256:<64 hex digits>
png /scratch/out/reference.png (922266 bytes)
render-seconds <time> (information only)
```

Usage: `lumen render --out FILE [--width W] [--height H]`; the default size is 320 by 180.
Width and height are each `1..=32768` (the limit of Bunny's camera) with at most `2^27`
pixels in all (a 512 MiB RGBA8 buffer, enough for 16K at 15360 by 8640). Bad arguments exit
with status 2 and the usage text; a refused size or an unwritable output path exits with
status 1 and a message. The output file is written after the render, so nothing is written
when the render fails.

The PNG is uncompressed (see below), so it is about four bytes per pixel: 922 266 bytes at
640 by 360, and 33 188 430 bytes (about 33 MB) at 3840 by 2160.

## The reference scene

`lumen-reference-scene/1` is defined in code in `crates/lumen/src/scene.rs`, entirely in Q32.32
integers with no floating point and no randomness from the host. It is made of triangles only,
so that a Geordi `geordi-ray-traced-scene/1` artifact can describe exactly the same scene
(flyingrobots/lumen#6). In primitive order:

1. the ground: two triangles at `y = -1.125`, spanning `-16..=16` on X and Z;
2. a back wall: two triangles at `z = -4`, `x = -3..=3`, `y = -1.125..=2`, facing the camera;
3. an octahedron of radius 1 centred at `(2.25, -0.125, 0.5)`, standing on a vertex;
4. a bumpy sphere around the origin, radius 1.0 to 1.04: a cube subdivided 48 by 48 per face and
   pushed out with Bunny's `FixedVec3::normalize`, adapted from Bunny's scene benchmark
   (27 648 triangles).

The camera stands at `(0, 1, 5)` looking along `(0, -0.25, -1)` with up `+Y` and a vertical
half-angle whose tangent is 0.5. Every vertex coordinate is a multiple of `2^-16` below 256
units, so it is exactly an `f32` and survives Geordi's `bunny-ply-binary-le-f32/1` mesh layout
unchanged; the sphere's vertices are floored to that grid.

The primitive order matters: of two triangles hit at the same distance, the lower index wins.
**Any change to the scene's triangles, their order or the camera placement is a new scene
version, and changes every image hash.**

## Shading

`lumen-normal-shading/1` (`crates/lumen/src/render.rs`, `shade`):

- a ray that hits nothing has the linear radiance `(0, 0, 0.25)`, code `(0, 0, 137)`;
- a ray that hits a triangle takes its geometric normal `normalize(cross(v1 - v0, v2 - v0))`,
  computed once per triangle with Bunny's `FixedVec3` operations (each product rounded to
  nearest, ties to even; the length the floor of the exact square root; each component divided
  with the same rounding), negates it when the ray hits the back face, and maps each component
  `n` to `(n + 1) / 2` (red from X, green from Y, blue from Z). The halving is Bunny's Q32.32
  multiplication by 0.5.

So a face whose normal points along `+Z` toward the camera is `(188, 188, 255)` and the ground,
`+Y`, is `(188, 255, 188)`. This is a debugging view, not lighting: lighting comes with Bunny's
shading step (flyingrobots/bunny#160) in flyingrobots/lumen#3.

## Image hash and PNG

The image hash is `sha256:` followed by the lowercase hex SHA-256 of the raw RGBA8 bytes
(row-major, row 0 at the top, alpha 255), as Geordi's ray-traced scene profile specifies. Width
and height are not part of it. The PNG file is only a container for those bytes; its own hash is
not a contract value.

**PNG encoder: hand-written, stored deflate blocks** (`crates/lumen/src/png.rs`). It writes
8-bit RGBA with filter type 0 on every row and the zlib stream as uncompressed stored blocks of
at most 65 535 bytes, one `IDAT` chunk each. Reasons:

- the contract is the hash of the raw bytes, so compression buys only file size;
- it needs no dependency and about 200 lines with its documentation, CRC-32 and Adler-32 included;
- its output bytes are a fixed function of the pixels, with no compression level or library
  version that could change them;
- it streams, holding one block rather than a second copy of the image.

The cost is size: about four bytes per pixel. A compressing encoder crate would make smaller
files but bring a dependency tree whose output bytes can change between versions. If file size
starts to matter, compression can be added later without touching the image hash.

**SHA-256: hand-written** (`crates/lumen/src/sha256.rs`), for the same reasons: no dependency,
short, and checked against the published FIPS 180-2 vectors and digests from Python's `hashlib`
around the padding boundaries.

## Determinism

The claim: the same scene at the same size gives the same RGBA bytes, and so the same image hash,
on every run, every platform and every build profile. Every value that decides a pixel is
integer arithmetic, from Bunny (camera rays, the BVH and its traversal, the exact triangle test,
normals, the tone curve and quantization) or exact in lumen (adding one to a normal component,
the face flip). There is no floating point in the render and nothing from the host enters it.

How it is tested (`crates/lumen/tests/`):

- **Two runs**: the same image rendered twice in one process, and by two separate `lumen render`
  processes, compared byte for byte along with the printed hash.
- **Against brute force**: for every pixel of a 16 by 16 and a 32 by 18 image, the renderer's
  hit (primitive, distance and facing) equals a test of every triangle in the scene, Bunny's own
  oracle applied to lumen's scene and rays.
- **Hand-derived pixels**: a one-triangle scene whose 3 by 3 image is derived by hand, with both
  windings; and in the reference scene, every pixel whose ray hits the ground, the wall, the
  octahedron or nothing has the colour derived by hand from its normal.
- **Golden hash**: the 96 by 54 reference image has a recorded hash, together with the scene,
  shading, tone curve and Bunny identities. It must be identical on every platform: it was first
  recorded on aarch64 Linux, and CI checks it on x86-64 Linux in both the test and release
  profiles. Updating it is legitimate only when one of those identities changes.

## Bunny dependency

Lumen uses Bunny's `bunny-num`, `bunny-linalg`, `bunny-geom`, `bunny-query`, `bunny-broadphase`
and `bunny-optics` at version 0.7.0. That version is tagged but not yet published on crates.io,
so the crates come in as git dependencies pinned to the full commit
`7fdd2e4926041b0a226567e0a6e1ec51e522304f` of `https://github.com/flyingrobots/bunny`. They move
to the crates.io registry once 0.7.0 is published there. `BUNNY_REV` in `crates/lumen/src/lib.rs`
records the commit, and a test checks that `Cargo.lock` resolves every Bunny crate to it.

Dependencies build with `opt-level = 2` even in debug builds and tests, so the brute-force tests
and a default-size render take seconds; lumen's own code keeps the debug profile.

## Build and test

Builds and tests run in the shared containers through `devrun`, never on the host
(see `~/git/devcontainers/README.md`). The toolchain is pinned to 1.96.0 in
`rust-toolchain.toml`, the version of the container image.

```sh
D=~/git/devcontainers/bin/devrun
python3 $D lumen ~/git/lumen --worktree --fetch -- cargo fmt --all --check
python3 $D lumen ~/git/lumen --worktree -- cargo clippy --workspace --all-targets --locked -- -D warnings
python3 $D lumen ~/git/lumen --worktree -- cargo test --locked --workspace
```

`--fetch` is needed on the first run in a worktree; it downloads the Bunny git dependency, and
later phases build offline. Use `--commit HEAD` instead of `--worktree` for evidence pinned to a
commit.

Lint posture mirrors Bunny: `unsafe` is forbidden, clippy `all` is denied and
`pedantic` warns, and CI runs with warnings denied.

## License

Apache-2.0. See [LICENSE](LICENSE).
