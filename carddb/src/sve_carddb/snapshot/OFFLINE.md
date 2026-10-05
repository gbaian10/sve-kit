# Offline regional supplements

`snapshot export-offline` composes the two launch regions from pinned card batches.
It reuses the identity, product, text-observation and supplemental importers; no
crawler settings, latest cache or live manifest are opened. Card IDs remain
manually adopted. Raw card numbers are matched only within their own region.

The recipe is JSON matching `sve_carddb.snapshot.offline.Inputs`: `repo`, `archive`,
`store_id`, sorted `sources` for `en` then `jp` (each has `region`, `card_batch`,
`image_batch`, `parser_version`), `revision`, `as_of`,
`data_version`, `published_at`, `feedback_url`, `grammar_version` and
`normalizer_version`. Vocabulary and UI languages are derived from the complete
`authored/catalog-adoptions` entry at the pinned revision by
`catalog.current.prepare` and written by `catalog.current.populate`. The catalog
index and every shard must use format 2. No caller vocabulary JSON or language
settings are accepted. Adopted EN and JA languages are required. The complete
existing `authored/translations` entry is explicitly enabled with its current
format 2 glossary. `compile_current_build` compiles the current catalog and
glossary evidence tables for both the initial build and bundle reconstruction.

Authored input bytes are checked against the immutable Git revision. Catalog
and glossary frozen evidence is validated against the current `BuildContext`
and its parser recipes. Their complete expected source-use closure is checked
independently of database insertion; missing imported catalog or glossary uses
fail before preview or bundle publication. Glossary source uses are collected
from current terms, concepts, choices and vocabulary values, including their
source spans and same-concept evidence.

Japanese effect text and flavor are translated in the same transaction. The
current templates, parameter rules and glossary are read at the pinned revision
and validated once; every Japanese main text or section of a face revision or
printed face whose exact source hash a template source covers is rendered whole
into a zh-Hant translation, selection and use. An uncovered fragment, invalid
placeholder, unresolved parameter or missing reference label keeps the whole field
original. Effect and flavor eligibility depends only on the source hash, not on
`printed_text_state`; unconfirmed card identities stay original. The report's
`effect_translations` counts fields, translated, original and low-confidence
results with fallback reason codes, and `flavor_translations` counts applied and
unused entries; neither repeats card text.

The build itself pins `catalog_source_recipes`, `translation_recipes`,
all package Python dependencies except generated `_version.py`, `uv.lock` and
`pyproject.toml`. Translation recipes cover EN, JP, sv1 and svwb frozen projections;
they do not grant digital same-card eligibility. Missing
trait/title or other raw-field adoptions fail closed rather than generating codes.
Image batches always verify authored correction evidence. Image publication is
optional and requires the two explicit image roots below; EN card text remains
the original EN observation, with no new translation workflow.

```bash
sve-carddb snapshot export-offline --inputs recipe.json \
  --preview-dir /path/to/public-preview \
  --private-dir /path/to/preview-private \
  --bundle-dir /path/to/private-build-bundle
```

All output roots must be disjoint from the protected repo, archive and recipe.
`--preview-dir` receives only public snapshot files and images. `--private-dir`
is a separate, persistent directory for `inputs/<hash>.json`,
`reports/<manifest-hash>.json` and `media-state.json`; keep it and back it up.
The bundle is a new private directory containing SQLite, inputs, report and seal.
Its complete source-use closure and archive pins are independently checked by
`publish_bundle`; `verify_bundle` can recheck it. The snapshot writer verifies
both shard and text-union readback before switching the preview pointer.
The bundle and private directory are never uploaded.

This remains a `preview-` candidate, with no formal index or activation (#34).
`export-offline` is the only export command. It selects EN and JP and permits
card-page Q&A and related links. The recipe retains unknown source coverage: observations do not prove Q&A or
errata completeness, nor CR/restriction coverage. Without explicit image roots,
no image publication is claimed.

## Regional preview images

Pass an existing private image library and recipe cache directory together.
For each pinned image batch, `export-offline` loads `authored/image-crops.yaml`
from the working tree, so uncommitted box edits apply, and calls
`build_regional_assets`. A verified five-size cache hit is reused; a missing or
stale entry (new source bytes, crop box or recipe) is encoded into the library
and cache. A new crop gets a new cache key, so an old crop is never substituted.
Keep the JP and EN sealed batches independent, and retain each original PNG.

```bash
sve-carddb snapshot export-offline --inputs recipe.json \
  --preview-dir /path/to/public-preview \
  --private-dir /path/to/preview-private \
  --bundle-dir /path/to/new-private-build-bundle \
  --image-assets-dir /path/to/private-image-library \
  --image-cache-dir /path/to/private-recipe-cache \
  --workers 2
```

`--workers` defaults to 2 and accepts 1 to 4; output bytes do not depend on it.
stdout `image_execution` reports wall time, cache hits, new encodings and the
summed per-image reuse and encoding times, which overlap with several workers.
The builder requires all current members of both pinned image batches,
verifies PNG hashes and oriented dimensions, and binds pages using the region's
extractor, exact original `img src` and adopted `source_face_map`. Card-number
suffixes do not infer cross-region identity. Source uses are retained separately
as `jp_image_link` / `en_image_link` and `jp_image_variant` / `en_image_variant`.

The same transaction and private bundle replay include the image rows, complete
source-use closure, exact recipe and crop adoption dependencies. Snapshot output
copies only referenced content-addressed WebP blobs; orphan library entries,
original PNGs and recipe caches stay private. Reports include applied/unused
crops, annotation mismatches and verified reprint candidates in both regions.
Available assets remove the image integration gate from this preview report;
formal activation and source coverage still require their existing gates.
`r2 upload-v2` uploads the preview root's current manifest closure and images to
the development bucket as a `preview-` index entry; see
[the upload guide](../r2_upload/v2/README.md).

## Supplemental capabilities

`Decisions.supplemental_restrictions` carries the typed result of
`require_card_extras_ready` into projection. `errata_current_pending` and
`source_printing_missing` are merged with existing reasons in
`card_engine_support.region_blocks`. They do not remove cards, faces, adopted
printings, route identities or default candidates. A pending erratum blocks
its card-region even when an older current revision is present. Private reports
retain affected face IDs, issue IDs, exact source identifiers and unresolved URLs;
public support exposes only region and reason codes. `errata_source_missing` additionally
marks a reference whose formal announcement evidence has not been imported;
`errata_current_pending` with available evidence marks corrected wording awaiting
verification. A sealed but unparsed body still needs C before it is available in
the public contract.

Reskin regions are recomputed after the source and current graph exists, using
`applicable_reskin_regions`. Unverified regions are removed only from that display
relation, without inheriting DSL or deck identity. Missing related targets and
same-card references remain in staging and their source uses are retained.

## Later errata input

The Python composition API accepts parsed `ErrataPage` inputs; the CLI does not
pretend that sealed announcement bodies have already been parsed. PR C supplies
that parser and its source adapter separately. `errata.official_url` is the exact
announcement URL; each version keeps `announced_on` nullable and each change keeps
`face_id`, `field`, `before` and `after`. Treat these as change fragments, retaining
all announcements. Date absence does not authorize substitution by fetch time,
effective date or an inferred year. No original printing text is reconstructed.

This recipe only reports pending errata restrictions; it has no confirmation input
or mechanism to clear them. PR C must first define the confirmation contract,
authorized maintainer, decision category/policy, complete correction scope and
freshness, together with its producer and consumer. Importing an announcement
alone does not approve current wording or clear any restriction.

The report uses counts and identifiers rather than official text. Unknown
coverage and pending checks are not a claim that there is no erratum. Real
publication requires complete adopted raw-field vocabulary, C's per-card corrected-text checks,
formal release gates and the separate Web consumer acceptance.

## Validation boundary

The native synthetic CLI test seals JP and EN card pages, registers two separate
card identities and product families, and exports a 2.0 preview and private
SQLite bundle using format 2 catalog and current glossary inputs. It uses the
actual frozen providers, compiler and catalog composition without adapters or
monkeypatching. Assertions cover both regions, physical card rows, current
authored provenance and retained catalog/glossary source uses.

Focused replay tests use an empty physical inventory to isolate current
catalog/glossary evidence and recipe pins. They independently check the complete
source-use closure and reject missing catalog or glossary uses before output.
Supplemental tests exercise card observations with mocked catalog composition.
These synthetic tests do not claim complete real-card, catalog and translation acceptance.
When real raw-field adoptions block the build, later-stage diagnostic runs must
be identified separately; an empty substitute for a blocked phase does not
prove the original phase or authorize a candidate for publication.

## 2.0 media previews

Select `--format-version 2.0.0` to export printing-owned media with permanent
image keys. The source library still uses verified content-addressed WebPs;
the isolated preview writes only the five display sizes to
`images/<size>/<int_id>[-f<ordinal>].webp`. `display_url` selects the card or art
version from media, never from the card number or an array position.

`media-state.json` in the private directory holds the high-water revision and
the last export's card/art comparison. Each export first raises the high-water
mark, so a failed export never reuses its number, and records its own groups
before switching the pointer. Pure text changes preserve both versions, art-only
changes preserve the card version, and a removed, restored or rebound image gets
the new revision. Exports into one private directory run one at a time. A missing
state file starts again at revision 1, which would reuse URLs a bucket may
already cache, so restore it from backup instead. Before overwriting an image
with different bytes, the writer removes the preview pointer; an interrupted
export leaves no pointer and must be rerun before uploading.

2.0 supports the same-name wire capability and rejects malformed nonempty links.
The existing offline recipe still keeps policy browsing private: its public
same-name links are empty. This preview does not certify policy browsing or
formal release readiness; policy links and the digital endpoint DB integration
remain separate. The 2.0 offline config itself always contains the two game URL
templates, with unknown service status; this does not perform a health check.
