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
`authored/catalog-adoptions` entry at the pinned revision by `derive_catalog`.
No caller vocabulary JSON or language settings are accepted. Adopted EN and JA
languages are required; immutable receipts, source evidence and their complete
source-use closure are checked and included in the build. Missing trait/title or
other raw-field adoptions fail closed rather than generating codes.
Image batches verify authored correction evidence, rather than publishing assets.

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
The old `snapshot export` stays JP-only and still rejects ancillary data. Only
`export-offline` selects EN and JP and permits card-page Q&A and related links.
Both recipes retain unknown source coverage: observations do not prove Q&A or
errata completeness, nor CR/restriction coverage. No image publication is claimed.

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
