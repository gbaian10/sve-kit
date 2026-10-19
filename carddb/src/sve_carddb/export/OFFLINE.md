# Offline regional supplements

`snapshot export-offline` composes the two launch regions from pinned card batches.
It reuses the identity, product, text-observation and supplemental importers; no
crawler settings, latest cache or live manifest are opened. Card IDs remain
manually adopted. Raw card numbers are matched only within their own region.

The recipe is JSON matching `sve_carddb.workflows.offline.Inputs`: `repo`, `archive`,
`store_id`, `source_mode` (defaults to `parsed`), sorted `sources` for `en` then `jp`
(each has `region`, `card_batch`,
`image_batch`, `parser_version`), `revision`, `as_of`,
`data_version`, `published_at`, `feedback_url`, `grammar_version` and
`normalizer_version`. Vocabulary and UI languages come from the current working
files in `authored/catalog/adoptions`. The catalog is prepared once and the same
checked plan supplies vocabulary, languages and DB rows. Offline catalogs use
format 2 shards; adopted EN and JA languages are required. The current format 2
translation entry is loaded once for glossary, templates and name overrides.
Readers scan only the dedicated data directories, validate types, size limits,
unique keys and references, then sort records. Shard numbers need not be contiguous
and records need not arrive sorted. Permanent registry allocation order is unchanged.

Optional `registry_review_inputs` pins `{root,batch_id,inventory_sha256,
coverage_hash,bytes,as_of}` to a permanent authored-layout §3.6 registry batch.
The historical reviews use `2026-09-28-identity-confirmed-v1`, never a draft copy.
Only fixed `jp.jsonl`, `inventory.json` and `SHA256SUMS` are read: inventory bytes,
both JP entries and the entire JSONL hash/size must match. Explicit broken pins
abort; missing pins exclude reviews. Coverage matches `jp`, the exact v1 scope,
the original date and hash. All registry JP printing numbers must be contained
in the historical set, or `review_scope_outdated` excludes reviews and the report
lists `uncovered_jp_card_numbers`. New JP cards need review renewal. EN source,
identity and current checks remain in force, without granting EN DSL support.

Text and image builds share the same provider factory. F1 configuration and the
plan report retain the verified pin without the private root or inventory's
`predecessor`/`copied_from` paths. The artifact does not create official HTTP
source records; review sources still point to authored records. Output roots must
also be disjoint from the permanent review input root.

The revision identifies the checkout for provenance; uncommitted authored edits
apply. Default builds read admitted `card-data/` and apply current configuration,
exact text, source language and owner applicability before projection. They do not
revalidate input schema, hashes, closure or history. Local card-data edits must pass
`parsed-cards validate` before building. Generation checks sealed/raw closure;
the data CI checks parsed schema, hashes, references and history without raw files.
Explicit `source_mode: archive` retains sealed/raw verification and parsing, never
as a fallback. Other-game APIs and image archives retain their raw verification.
See [the build input contract](../../../../docs/schema/ingest/parsed-card-inputs.md#建置入口).
The input record summarizes actual source uses.

Japanese effect text and flavor are translated in the same transaction. The
current templates, parameter rules and glossary are read from the working tree
and validated once against source positions generated from the recipe's JP card
batch (Git stores no template source inventory); every Japanese main text or
section of a face revision or printed face whose exact source hash a template
source covers is rendered whole
into a zh-Hant translation, selection and use. An uncovered fragment, invalid
placeholder, unresolved parameter or missing reference label keeps the whole field
original. Effect and flavor eligibility depends only on the source hash, not on
`printed_text_state`; unconfirmed card identities stay original. The report's
`effect_translations` counts fields, translated, original and low-confidence
results with fallback reason codes, and `flavor_translations` counts applied and
unused entries; neither repeats card text.

Translation parsers cover EN, JP, sv1 and svwb frozen projections; they do not
grant digital same-card eligibility. Missing trait/title or other raw-field
adoptions fail rather than generating codes.
The pinned image index supplies exact authored correction evidence. Image metadata is
optional and requires an explicit index; EN card text remains
the original EN observation, with no new translation workflow.

```bash
sve-carddb snapshot export-offline --inputs recipe.json \
  --preview-dir /path/to/public-preview \
  --private-dir /path/to/preview-private \
  --bundle-dir /path/to/private-build-bundle --image-index "/private/image-index/<sha256>.json"
```

`--preview-dir` can read `SVE_EXPORT_DIR`; `--private-dir` can read
`SVE_CARDDB_PRIVATE_DIR`. CLI options take precedence, neither root has a default,
and both must be non-empty absolute paths. `--inputs` and `--bundle-dir` remain
explicit. `SVE_PREVIEW_DIR` configures only the Web `/cdn-preview` root.

All output roots must be disjoint from the protected repo, archive and recipe.
`--preview-dir` receives only public snapshot JSON files. `--private-dir`
is a separate, persistent directory for `inputs/<hash>.json`,
`reports/<manifest-hash>.json`; image state belongs to the independent image producer.
The build directory is new and private. It contains `build.sqlite`, `inputs.json`
and `report.json`. The database is backed up from the completed transaction, with
no second population or build seal. Files are written in a temporary directory,
fsynced and installed through a no-overwrite rename. The snapshot writer verifies
the shard join before switching the preview pointer.
The build directory and private directory are never uploaded.

This remains a `preview-` candidate, with no formal index or activation (#34).
`export-offline` is the only export command. It selects EN and JP and permits
card-page Q&A and related links. The recipe retains unknown source coverage: observations do not prove Q&A or
errata completeness, nor CR/restriction coverage. Without an explicit image index,
no image publication is claimed. With an image index, the private report omits
`image publication (#35)` from `incomplete_formal_gates`, as in the image-enabled
preview baseline; the remaining formal gates still apply.

## Regional preview images

Run `sve-carddb images build-index` independently, then supply its immutable index
with `snapshot export-offline --image-index`. The text build checks the exact
regional image pins, printing/face permanent identity and parsed original img src.
It never opens PNGs, WebPs, conversion caches or media state. Image rows are joined
after the text database projection, keeping that database free of image variants.
The public media fields and event versions remain unchanged.

See the [image index contract](../../../../docs/schema/images/image-index.md) for
producer roots, cache keys, complete verification and state migration. Publication
still verifies all images; `sve-publish upload --image-dir` supplies the independent
fixed-key image output.

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
card identities and product families, and exports a 4.0 preview and private
SQLite bundle using format 2 catalog and current glossary inputs. It uses the
actual frozen providers, compiler and catalog composition without adapters or
monkeypatching. Assertions cover both regions, physical card rows, current
authored provenance and retained catalog/glossary source uses.

Focused tests cover working-tree edits, shard gaps, unordered records, duplicate
keys, exact source evidence and owner mismatches. Supplemental tests exercise card
observations with mocked catalog composition. Synthetic tests do not claim complete
real-card, catalog and translation acceptance.

## 4.0 media previews

Select `--format-version 4.0.0` to export printing-owned media with permanent
image keys. The source library still uses verified content-addressed WebPs;
the image producer writes the five display sizes to
`images/<size>/<int_id>[-f<ordinal>].webp`. `display_url` selects the card or art
version from media, never from the card number or an array position.

The independent image producer owns persistent `media-state.json`, reserves before
an image build and commits only after images and index are written. Pure text
exports consume the index's versions without allocating or committing any state.
Failed image builds burn their reservation; restore missing state from backup.

4.0 supports the same-name wire capability and rejects malformed nonempty links.
The existing offline recipe still keeps policy browsing private: its public
same-name links are empty. This preview does not certify policy browsing or
formal release readiness; policy links and the digital endpoint DB integration
remain separate. The 4.0 offline config itself always contains the two game URL
templates, with unknown service status; this does not perform a health check.
