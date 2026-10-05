# Regional image assets

API summary. Contracts live in `docs/schema/image-variants.md` (recipe, sizes, crop)
and `docs/schema/image-crop-overrides.md` (adopted crop boxes).

- `build_regional_assets(FrozenSources, PreviewRoots, region=, crops=, workers=)`
  converts every **current** source of one exclusively `jp` or `en` image batch to
  five WebP files under `images/sha256/<prefix>/<digest>.webp`, plus a private
  recipe cache. Verified cache hits are reused and only misses are encoded. It
  reads only `FrozenSources`, never live manifests or the network, and gives
  identical bytes for one to four workers. Roots must be absolute and disjoint;
  symlinks fail.
- `plan_regional_images(db, plan, cards, region=)` reads each face's actual `img src`
  through the adopted `source_face_map`, never from card numbers or face order.
- `populate_assets(db, build, references, preview)` writes `image_asset`,
  `image_variant` and `printing_image` rows. A URL absent from the image batch stays
  pending/unfetched with no variants; corrupt archive bytes abort.
- `verify_asset_sources(build, stores, crops=)` rechecks source size, oriented
  dimensions and crop box against the sealed PNGs.
- `crop_report(crops, build, references, db)` counts applied crop sources, lists
  unused rows and annotation mismatches, and flags reprints of the same card/face
  whose source has no crop. It never inherits a box or blocks a build.

Crops come from `image_crops.load_image_crops` and are selected by
`(source_key, source_sha256)`. Reports never include official card text.
The `export-offline` command is the only caller: it encodes whatever the cache
lacks and wires the result into a snapshot preview; see `snapshot/OFFLINE.md`.
