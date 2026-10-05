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
  --preview-dir /path/to/private-preview --cdn-dir /path/to/formal-cdn \
  --bundle-dir /path/to/private-build-bundle
```

All output roots must be disjoint from protected inputs and formal output. The
bundle is a new private directory containing SQLite, inputs, report and seal.
Its complete source-use closure and archive pins are independently checked by
`publish_bundle`; `verify_bundle` can recheck it. The snapshot writer verifies
both shard and text-union readback before switching the private preview pointer.
The bundle and reports are private: do not deploy them with public snapshots.

This remains a `preview-` candidate, with no formal index or activation (#34).
`export-offline` is the only export command. It selects EN and JP and permits
card-page Q&A and related links. The recipe retains unknown source coverage: observations do not prove Q&A or
errata completeness, nor CR/restriction coverage. Without explicit image roots,
no image publication is claimed.

## Regional preview images

Prepare a separate private image library and recipe cache before exporting.
`build_regional_assets(FrozenSources(...), roots, region=pin.region, crops=crops,
workers=2)` converts one exclusively regional image batch. Load the complete
`authored/image-crops` closure at the same revision, including unused records.
Keep the JP and EN sealed batches independent, and retain each original PNG.

```bash
sve-carddb snapshot export-offline --inputs recipe.json \
  --preview-dir /path/to/new-preview --cdn-dir /path/to/formal-cdn \
  --bundle-dir /path/to/new-private-build-bundle \
  --image-assets-dir /path/to/private-image-library \
  --image-cache-dir /path/to/private-recipe-cache
```

The two image options must be provided together. CLI export only reuses validated
five-size caches, with two workers, and never silently encodes or substitutes an
old crop. The builder requires all current members of both pinned image batches,
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
Preview exports remain local. R2 publication uses `r2 upload-v2` with a formally
gated frozen snapshot 2.0 release and its existing ledger/checkpoint inputs;
see [the publication guide](../r2_upload/v2/README.md).

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

`private/media-revisions.jsonl` reserves local preview revisions under an exclusive
lock and fsyncs before producing a candidate. Failed numbers remain reserved.
`private/media-committed.json` supplies the last successful card/art comparison;
removed bindings retain tombstones. Pure text changes preserve both versions,
art-only changes preserve the card version, and restoration uses a new revision.
These files are private preview state, not an R2
publisher, backed-up production allocator, formal release receipt or CDN check.
Do not upload `private/` or `reports/`.

2.0 supports the same-name wire capability and rejects malformed nonempty links.
The existing offline recipe still keeps policy browsing private: its public
same-name links are empty. This preview does not certify policy browsing or
formal release readiness; policy links and the digital endpoint DB integration
remain separate. The 2.0 offline config itself always contains the two game URL
templates, with unknown service status; this does not perform a health check.
