# Regional image assets

API summary. Contracts live in `docs/schema/images/image-variants.md` (recipe, sizes, crop)
and `docs/schema/images/image-crop-overrides.md` (adopted crop boxes).

- `build_regional_assets(FrozenSources, PreviewRoots, region=, crops=, workers=)`
  converts every **current** source of one exclusively `jp` or `en` image batch to
  five WebP files under `images/sha256/<prefix>/<digest>.webp`, plus a private
  recipe cache. Verified cache hits are reused and only misses are encoded. It
  reads only `FrozenSources`, never live manifests or the network, and gives
  identical bytes for one to four workers. Roots must be absolute and disjoint;
  symlinks fail.
- `workflows.image_index.media_projection(texts, images)` binds each face's actual
  `img src` through the adopted `source_face_map` and permanent printing/face IDs.
  A URL absent from its regional image batch stays pending/unfetched with no variants.
- `crop_report(crops, build, bindings)` counts applied crop sources, lists unused
  rows and annotation mismatches, and flags verified reprints of the same card/face
  whose source has no crop. Ownership comes from the media projection's adopted
  identity. It never inherits a box or blocks a build. `images build-index` writes
  all diagnostics to `image-report.json` under its private root and prints their
  counts in the command's one-line JSON summary.

Crops come from `sve_carddb.images.crops.load_image_crops`, which reads
`authored/images/crops.yaml` from the working tree, and are selected by
`(source_key, source_sha256)`. Reports never include official card text.
The independent `images build-index` command encodes cache misses and assigns
media event versions. Text `snapshot export-offline --image-index` reads only its
pinned metadata, without PNGs, WebPs, conversion caches or media state. Daily
image builds reuse archived hashes with size checks; `--verify-all` performs a
complete source and WebP scrub. See the [index contract](../../../../docs/schema/images/image-index.md).
