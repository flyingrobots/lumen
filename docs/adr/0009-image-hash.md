# ADR 0009: The image hash

Status: accepted. Taken in flyingrobots/lumen#2 (commit `b191a1d`, 2026-11-13) and recorded here
on 2026-11-14 for flyingrobots/lumen#25, from the code and tests as they are; nothing changes.

## Context

Lumen's promise is that the same scene and settings give the same image everywhere. That needs
one value that names an image exactly, so that two renders, two platforms or a render and a
golden constant can be compared without comparing files. Three choices define it: what bytes are
hashed, with which function, and how the digest is written. The image hash is a contract value:
it is printed by `lumen render` (the `image` line), returned by `Rgba8Image::hash`, pinned as the
golden constants every Determinism job checks, and recorded in the measurement records.

## Decision

- **What is hashed: the decoded pixels, not a file.** The input is the image's raw RGBA8 bytes:
  four bytes per pixel (red, green, blue, alpha), row-major, row 0 at the top, each row left to
  right, alpha always 255 (Bunny's `bunny-rgba8-opaque/1`). Nothing else is hashed: not the PNG
  file, not the width or the height, not the provenance.
- **The function: SHA-256** (FIPS 180-4), over those bytes as one message.
- **The form: `sha256:` followed by the digest as 64 lowercase hexadecimal digits**, 71
  characters in all. `Rgba8Image::hash` returns it; the command prints it unchanged.
- **The SHA-256 is lumen's own** (`crates/lumen/src/sha256.rs`, internal to the crate), so the
  hash needs no dependency (Lumen takes no crates.io crate).

## Why

- **The pixels are the result; a file is a container.** Hashing the PNG would tie the contract to
  the encoder: its filter choice, block layout or a future compression level would change the
  hash of an unchanged image. Hashing the raw bytes lets the PNG encoder change, or compression
  be added, without touching any recorded hash. The PNG's own hash is not a contract value.
- **It is the form Geordi specifies** for ray-traced renders (Geordi's
  `docs/design/2026-11-fixed-point-and-ray-traced-scene-profiles.md`, item B6: "SHA-256 over the
  RGBA bytes. PNG bytes are not part of the contract", and its image hash section: "`sha256:`
  followed by the lowercase hex SHA-256 of exactly those RGBA bytes"), so a hash computed by
  Lumen and one computed by a Geordi runtime for the same image can be compared directly.
- **No size in the hash.** The size is a provenance field of its own (`size`, `pixels`), known
  before the render; the hash is a result. A consequence, true of the code as written and not
  tested: two images of different shapes with the same bytes (a 2 by 1 and a 1 by 2 image) have
  the same hash, so a hash identifies an image only together with its size. Geordi's design
  says the same and carries the size in its render receipt.
- **SHA-256** is fixed, widely implemented and checkable outside Lumen with standard tools
  (`sha256sum`, Python's `hashlib`), which is how the measurement records check exported images.
- **A prefixed, lowercase, hexadecimal string** names the function in the value, so a later
  change of function cannot be confused with a changed image, and it is the same form the scene
  content identity uses (ADR 0005).

## Evidence

- The doc example of `Rgba8Image::hash` (`crates/lumen/src/image.rs`): one opaque black pixel,
  `00 00 00 ff`, has the published digest of those four bytes.
- `crates/lumen/src/sha256.rs`: the published FIPS 180-2 vectors, digests from Python's `hashlib`
  around the padding boundaries, and any split of any input giving the digest of the whole.
- `tests/cli.rs::the_printed_hash_ray_count_and_file_are_the_librarys`: the command prints the
  library's hash.
- The golden hashes (`crates/lumen/tests/golden/mod.rs`), one value per scene for every platform
  and profile, are this hash of 96 by 54 renders. The requirement-to-evidence map in
  [the explanation](../explanation.md#requirements-and-their-evidence) lists them as R-HASH-1 and
  R-DET-1 to R-DET-4.

## Consequences

- Changing what is hashed, the function or the form is a new contract: every golden constant
  and every recorded hash would change, and this record would be superseded.
- A front end that wants to check a PNG file against a recorded hash decodes it to RGBA8 first.
- A reader comparing hashes must compare sizes too (above).

## Rejected

- **Hashing the PNG file**: ties the contract to the encoder (above).
- **Including the width and height**: duplicates provenance fields and departs from Geordi's
  profile.
- **A faster non-cryptographic hash**: the hash is computed once per render, about 0.1 s of a
  6 s 4K render in the first 4K record (`hash-seconds`,
  [2026-11-13](../measurements/2026-11-13-4k-reference.md)), and a collision-resistant digest is
  what makes a recorded value trustworthy as a name.
