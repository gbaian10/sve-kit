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

Both image APIs require an explicit adopted crop collection, even when empty.
First call `image_crops.load_image_crops(authored_root,
authored_revision=<full Git SHA>)` and pass its result as `crops=` to
`build_jp_assets` and `publish_jp_image_bundle`. The loader reads all shards
in `authored/image-crops/`, without an index, and checks their complete
file set and exact bytes against the pinned revision. It validates EN rows too;
the JP importer still does not convert or bind EN sources. An absent directory
at that revision is an explicitly empty closure.

Selection uses `(source_key, source_sha256)`, never the card number annotations.
The shared resolver accepts verified JP or EN image descriptors, derives the
existing conversion `img:v1:` ID and leaves public `img:binding:` IDs unchanged.
A known resource with new unadopted bytes fails instead of reverting to a default
box. A new adopted hash may explicitly restore the default box. Overrides change
only the integer crop; they add no rotation. New crop keys produce new art blobs
while the three card variants retain their content hashes.

`plan_jp_images` takes an already verified identity `PreviewPlan`, its parent DB
and an explicitly pinned card-page batch. It uses the adopted `source_face_map`
to read each face's actual `img src`, preserving its spelling and resolving the
URL against the archived page using the crawler's existing URL canonicalization
(including spaces and existing percent escapes). It does not use logical face
order to guess source order, or card numbers to construct image URLs. The page source metadata, DB
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

Include `crops.dependencies()` in the build inputs and
`{"image_crop_overrides": crops.configuration()}` in the configuration, alongside
the recipe pin. The bundle consumer rechecks these pins and independently
compares each source's actual `VariantSet.crop_box` with its adopted or default
box using verified oriented dimensions. The snapshot preview's image path loads
its own adopted closure, checks externally supplied image builds the same way,
and pins all crop files including unused rows. Text-only
previews do not load or pin crop inputs. The export CLI only reuses a complete
cache entry for the selected box; it never repairs missing artifacts.

Crop reports count distinct applied sources, list unused rows (including EN),
annotation mismatches and other-printing candidates for the same effective
permanent card/face. These diagnostics neither inherit boxes nor block builds.
`art_webp_review: pending_coordinator_review` records that approving the RGB crop
does not approve the encoded WebP output. Technical review and upload authorization
remain separate operations.

Image IDs bind source versions; bound DB asset IDs also retain exact original src
spellings. Different references or sizes may share identical content blobs.
Conversion reports count source images, variant metadata rows and unique files
separately, and report missing card numbers/page hashes and unmapped image hashes.
They never include official card text.

This API supplies local assets and DB rows to the snapshot preview. It does not
write snapshot version indexes or promote a preview to a formal release.
