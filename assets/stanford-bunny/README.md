# Stanford Bunny

**Asset**: `bun_zipper_res3.ply`, the Stanford bunny, used in lumen's reference scene
(`lumen-reference-scene/3`, see the repository README).

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
files' line endings, and a test (`ply::tests::the_committed_asset_has_geordis_hash`) checks the
PLY's SHA-256 with lumen's own `sha256` module.

## Mesh profile

- ASCII PLY 1.0, one `comment zipper output` line
- 1 889 vertices with properties `x y z confidence intensity` (lumen uses `x y z` only)
- 3 851 triangular faces (`property list uchar int vertex_indices`)
- Bounds min `[-0.0943643, 0.0334143, -0.0616721]`, max `[0.0609346, 0.184813, 0.0584651]`
  (as in Geordi's `bunny.mesh.json`)
- One coordinate in exponent form (`8.20736e-05`); the others are plain decimals

Lumen reads it with the interim reader in `crates/lumen/src/ply.rs`
(`lumen-ascii-ply-q32.32/1`), to be replaced by Bunny's ASCII PLY reader once it exists
(flyingrobots/bunny#247).
