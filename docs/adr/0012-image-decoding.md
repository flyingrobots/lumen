# ADR 0012: Image decoding lives in a dependency-free `lumen-image` crate

Status: accepted (flyingrobots/lumen#184). The decision was taken on 2026-11-15 and is recorded
here before any of the code exists; the crate is built by flyingrobots/lumen#195 to #199.

## Context

Lumen will need to read images: the texture maps of a model (the PBR bunny has seven, the
largest a 4096 by 4096 normal map) and the images that agents composing covers bring with them.
Both arrive as PNG or JPEG files and must be decoded to pixels at render time. Lumen has no
decoder today; it only writes PNG (stored deflate blocks).

Two rules of the repository decide how a decoder may be written:

- **No third-party crate.** R-DEP-1 in the
  [evidence map](../explanation.md#requirements-and-their-evidence) says Lumen takes Bunny's
  crates from one git repository, pinned by commit, and nothing from a registry. `Gates /
  cargo-deny` enforces it (`deny.toml`: `allow-registry = []`).
- **No float in the deterministic core.** [ADR 0006](0006-nondeterminism-gate.md) bans every
  float type, `clippy.toml` lists them as `float-type` bans, and each deterministic crate's root
  denies `clippy::float_arithmetic`.

A third reason is specific to JPEG. The standard does not fix the decoded pixels: it allows a
tolerance in the inverse discrete cosine transform (IDCT) and leaves chroma upsampling open, so
two correct decoders give slightly different pixels. Decoded pixels are what the image hash
names ([ADR 0009](0009-image-hash.md)), so a render that uses a decoded texture is only
reproducible if Lumen states the arithmetic that produced the pixels.

## Decision

- **PNG and JPEG are decoded by new code in a separate workspace crate, `lumen-image`.** It has
  no dependency, not even Bunny's, and is written without floating point.
- **The crate is a deterministic crate.** The gates of ADR 0006 apply to it as they do to
  `lumen`: the float and nondeterminism bans, `clippy::float_arithmetic` denied at its root, and
  the file-length limits. Its tasks name the gate commands in their acceptance (`xtask
  nondeterminism`, `clippy-bans`, `file-length`).
- **The rules the code follows**: typed errors that name the refused feature or the byte offset;
  every limit (output size, dimensions) taken from the caller and checked before allocating; no
  panic and no over-read on any input.
- **JPEG has a named identity, `lumen-jpeg-decode/1`**, which fixes the integer IDCT and the
  chroma upsampling rule for 4:4:4, 4:2:2 and 4:2:0, and the integer YCbCr to RGB conversion.
  The rules themselves are written down by #197; changing one makes a new identity.
- **Supported and refused features** (everything not supported is refused with a typed error
  that names it):

  | Format | Supported | Refused | Skipped without error |
  | --- | --- | --- | --- |
  | zlib stream (#195) | stored, fixed-Huffman and dynamic-Huffman blocks; the Adler-32 check; an output limit from the caller | gzip and raw-deflate framing; a stream that would exceed the limit (before allocating past it); a bad checksum | not applicable (decoding only: nothing compresses) |
  | PNG (#196) | 8-bit grey, RGB and RGBA, not interlaced; the five filters; CRC checks; limits on dimensions; decoded to RGBA8 | interlaced files, paletted files, 16-bit files, a bad CRC, a truncated file | gamma, ICC profiles and text chunks |
  | JPEG, baseline (#197) | 8-bit, Huffman-coded, sequential; 4:4:4, 4:2:2 and 4:2:0; greyscale; restart intervals; JFIF YCbCr; decoded to RGB8 | arithmetic coding, 12-bit samples, CMYK, malformed markers, truncated data | EXIF orientation (not applied) |
  | JPEG, progressive (#198, after baseline) | 8-bit Huffman progressive scans: spectral selection and successive approximation, DC and AC, end-of-band runs, feeding the baseline dequantization, IDCT and colour conversion | arithmetic coding, hierarchical and lossless modes, truncated scans | as for baseline |

- **Scope of the supported subset**: the decision's record says all seven of the PBR bunny's
  maps fall inside the baseline subset; that is a claim about files outside this repository and
  #196 and #197 check it by decoding them to recorded SHA-256 values.

## Why

- **The two rules forbid the one-line answer.** A registry decoder (`image`, `png`,
  `zune-jpeg`) fails R-DEP-1 at once, and the usual decoders use floating point or
  platform-specific fast paths, which the float ban refuses in the core.
- **A dependency's pixels are not Lumen's to state.** The image hash names the decoded pixels.
  With a dependency, a version bump could change them, and Lumen could not say which IDCT and
  upsampling it used. Its own integer code can, and `lumen-jpeg-decode/1` says it.
- **A separate crate** because the decoders need nothing from the renderer. The boundary keeps
  that true, and gives the fuzz targets of #199 one crate to attack. It also lets the code move
  to another repository later unchanged.
- **The decoders sit behind a small interface**, so replacing the implementation changes one
  crate; the decision is reversible.

## Consequences

- About 33 hours of work in five tasks: #195 (inflate, 6 h), #196 (PNG, 5 h), #197 (baseline
  JPEG, 10 h, the largest), #198 (progressive JPEG, 8 h), #199 (fuzz targets and a replayed
  corpus, 4 h).
- The new crate must be accepted by the gates of ADR 0006: the workspace-boundary rules there
  derive the deterministic crates from `lumen`'s dependencies and require every other member to
  be classified as tooling. How #195 satisfies them (the crate is not a dependency of `lumen`
  until something uses it) is that task's to settle; this record does not change those gates.
- Lumen takes on the correctness of its decoders and the fuzzing of hostile files, which a
  dependency would have shared with its users.
- Decoding is a decision about files, not a promise about all PNG or JPEG files: a file outside
  the table above is refused, not approximated.

## Rejected

| Alternative | Why not |
| --- | --- |
| The `image`, `png` or `zune-jpeg` crates | Breaks both rules, and ties the image hash to code Lumen does not control. |
| Decoders in Bunny's codec crate | Bunny holds math and mesh formats, and every decoder fix would need a pin move here. The crate boundary lets the code move there later unchanged. |
| Decode once offline and commit raw pixels | A raw 4096 by 4096 RGB map is 48 MiB, and agents composing covers bring images that must be decoded at render time. |
| A module inside the `lumen` crate | The decoders need nothing from the renderer; a separate crate keeps that true and gives the fuzzers one target. |
