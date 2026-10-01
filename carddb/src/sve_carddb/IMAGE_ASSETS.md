# Sealed JP image assets

`image_assets.build_jp_assets` converts every **current** source in an explicitly
pinned `jp:image` batch, including sources that do not yet have a printing binding.
It uses `FrozenSources`, never live manifests, latest files or the network.

Pass absolute, isolated `PreviewRoots(preview, cdn, cache)`. The preview root must
not equal, contain or be contained by the formal CDN root, cache or archive.
Symlink output roots and descendants fail. Encoding supports one to four worker
threads and returns identical bytes in serial and parallel builds. Production
runs should set `PYTEST_XDIST_AUTO_NUM_WORKERS=4` and `CARGO_BUILD_JOBS=4` as well.

Each source produces all five WebP variants under
`images/sha256/<prefix>/<digest>.webp`. The private recipe cache verifies source,
geometry and existing blobs before reuse, so rerunning after interruption keeps
complete blobs and resumes unfinished work. Neither original PNGs nor SQLite or
cache files enter the asset root. Old immutable blobs remain intact.

`plan_jp_images` takes an already verified identity `PreviewPlan`, its parent DB
and an explicitly pinned card-page batch. It uses the adopted `source_face_map`
to read each face's actual `img src`, preserving its spelling and resolving the
URL against the archived page. It does not use logical face order to guess source
order, or card numbers to construct image URLs. The page source metadata, DB
printing and complete face map must agree.

`populate_jp_assets` composes into the caller's transaction, after checking every
blob. Available sources become official approved `image_asset` rows, five
`image_variant` rows and exact `printing_image` bindings. A referenced URL absent
from the pinned image batch stays pending/unfetched with no variants. Missing or
corrupt archive bytes abort the build. This importer accepts only verified
**official JP** bindings; third-party review and withdrawn assets remain under
the existing DB contract and are not approved by this path.

`publish_jp_image_bundle` composes the parent population callback and image rows
with the existing four-file build bundle contract: `build.sqlite`, `inputs.json`,
`report.json`, `seal.json`. Its explicit build context must pin `image_recipe` to
`DEFAULT_RECIPE.version`. The private bundle destination is isolated from the
public asset/CDN roots and archive/cache roots. It verifies the exact frozen raw
size and oriented dimensions, writes assets first, validates all source uses and
DB constraints, then publishes a complete immutable bundle. Failures leave an
existing bundle intact. A new build uses a new bundle destination.

Image IDs bind source versions; bound DB asset IDs also retain exact original src
spellings. Different references or sizes may share identical content blobs.
Conversion reports count source images, variant metadata rows and unique files
separately, and report missing card numbers/page hashes and unmapped image hashes.
They never include official card text.

This API supplies local assets and DB rows. Projecting the public image manifest
and connecting it to the snapshot preview is separate work; it does not write
snapshot version indexes or promote a preview to a formal release.
