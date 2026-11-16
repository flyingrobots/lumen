# Stanford Bunny PBR

**Assets**, five files taken byte for byte from the author's archive:

- `model.dae`, a COLLADA 1.4.1 model of the bunny (5 696 triangles);
- `DefaultMaterial_albedo.jpg`, `DefaultMaterial_normal.png`, `DefaultMaterial_roughness.jpg` and
  `DefaultMaterial_metallic.jpg`, its four material maps.

Lumen will use them as a physically based material. Nothing reads them yet (flyingrobots/lumen#207
adds the files; converting the mesh is a later issue). They are not Lumen's work, and Lumen's
Apache-2.0 license does not apply to them; the terms are below. This is a different asset from
the Stanford scan in `assets/stanford-bunny/`.

## Source, license and attribution

- Page: <https://sketchfab.com/3d-models/stanford-bunny-pbr-42c9bdc4d27a418daa19b2d5ff690095>
- Title "Stanford Bunny PBR", by the Sketchfab user **hackmans**, published 2022-03-16
- License: **Creative Commons Attribution 4.0 International (CC BY 4.0)**,
  <http://creativecommons.org/licenses/by/4.0/>. It allows sharing and adapting, including for
  commercial use, if you credit the author, link the license and say whether you changed the
  material.
- Where the license was read: from Sketchfab's API on 2026-11-15. The download itself has no
  license file, so the license is not recorded in the archive.
- Derivation: the author's description says the model was remeshed from the Stanford Bunny in
  Morgan McGuire's Computer Graphics Archive, with the normal map baked from the source model.
  The original scan is the Stanford Bunny of the Stanford 3D Scanning Repository (see
  `assets/stanford-bunny/README.md`).
- Sketchfab lists 5 696 faces; the model in the archive has 5 696 triangles.

Attribution text to keep with the files wherever they are used:

> "Stanford Bunny PBR" by hackmans, <https://sketchfab.com/3d-models/stanford-bunny-pbr-42c9bdc4d27a418daa19b2d5ff690095>,
> licensed under CC BY 4.0 (<http://creativecommons.org/licenses/by/4.0/>). Derived from the
> Stanford Bunny as remeshed in Morgan McGuire's Computer Graphics Archive. Lumen has not
> changed the files.

## Provenance and hashes

The author's archive is `model.zip`, downloaded from Sketchfab: 17 183 409 bytes, SHA-256
`78122309dd9715a9c82a534b4713ef49416aa3e022e65c9dd60014e5dff9300b`. It was extracted with
`unzip` into an empty directory and the five files copied out. Each SHA-256 below was computed
with `shasum -a 256` on the committed file and equals that of the file inside the archive
(compared with `cmp`); the sizes are the archive's.

| File (path in the archive) | Bytes | SHA-256 |
| --- | ---: | --- |
| `model.dae` (`model/model.dae`) | 521 932 | `b4aa33df194c7bb55b32e915100e28676fc2b471448290cc0da53b366806a30e` |
| `DefaultMaterial_albedo.jpg` (`model/textures/`) | 1 853 444 | `26d1af1b1e3ee57bd79d852b9579dcbe47f025d0917c1a6b1e610ab50c2fef15` |
| `DefaultMaterial_normal.png` (`model/textures/`) | 14 218 747 | `24c60b788c6713ba7dd28663553d597eb664a4cf8dc64c23acf73137a6f4025a` |
| `DefaultMaterial_roughness.jpg` (`model/textures/`) | 542 796 | `b39f1cbbc1bf34ebcb01b08d844e62cf529e89dc074b996620ee9cbb35c43fb4` |
| `DefaultMaterial_metallic.jpg` (`model/textures/`) | 196 940 | `54548e60df5bbe94d8944c7736dc1edeccdc5569a7d756e0a8ffce0702960c9f` |

The root `.gitattributes` marks `assets/**` as `-text`, so Git never rewrites these files' bytes
(line endings or otherwise). Each map is 4096 by 4096 pixels.

Note that `model.dae` does not name the texture files; the material is set up by the user of the
model.

## What the maps contain

Measured on a 512-pixel downsample, so a few stray pixels could differ:

| Map | Values |
| --- | --- |
| Albedo | varied colour |
| Normal | varied; relief baked from the source model |
| Roughness | 204 to 255 of 255 |
| Metallic | all 0 |

So the bunny renders as a matte figure with fine relief; it will not look shiny.

## What was left out

The archive also has `DefaultMaterial_AO.jpg` (617 851 bytes), `DefaultMaterial_emissive.jpg`
(262 771 bytes) and `DefaultMaterial_opacity.jpg` (196 940 bytes), 1 077 562 bytes in all.
They are not committed:

- ambient occlusion is baked lighting, which a path tracer computes for itself;
- emissive is all 0 and opacity is all 255 (measured as above): they carry no information.

## What the asset costs the repository

The five files are 17 333 859 bytes (about 17.3 MB, 16.5 MiB). The largest, the normal map, is
14.2 MB, under GitHub's 100 MB file limit. The JPEG and PNG files are already compressed, so a
clone grows by about the same amount, and history keeps it permanently. No Git LFS is used. The
library reads no file and embeds none, so no Lumen program grows: nothing refers to the files.
