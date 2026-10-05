# authored

Human-maintained data and permanent identity allocations, read by `carddb` at build time.

- `registry/card/`, `registry/face/`, `registry/printing/` — permanent identities and manually confirmed JP/EN membership, filed by immutable owner
- `ids/index.yaml`, `ids/<owner>/*.yaml` — allocation policy and cursors, and append-only printing `int_id` allocations in per-region ranges
- `registry/region_mapping_review/` — confirmed absence of Japanese counterparts, with scope and date
- `registry/art/` — reviewed English original-art uses
- `registry/card_related/` — source-pinned reskin relationships
- `registry/source_correction/` — confirmed corrections and separately marked review candidates; raw observations remain unchanged
- `effects/` — effect data in the DSL defined by `../dsl/`
- `rulings/` — evidence-backed interpretations; see `../docs/adr/0011-rulings-evidence.md`
- `translations/index.yaml`, `translations/glossary/<filing_key>/*.yaml` — checksummed glossary concepts and editable current translations
- `flavor-translations/<hash digit>.yaml` — whole-paragraph flavor translations keyed by the source text's SHA-256; see [flavor translation](../docs/schema/flavor-translation.md)

The identity registry format is defined in [authored layout](../docs/schema/authored-layout.md).
The former `card-ids.yaml` proposal is replaced by `registry/` and `ids/`.
Run the offline generator with `python -m sve_carddb.registry --help` through the carddb uv environment.
Never regenerate identity IDs from sorting, rewrite an existing registry record in place,
or treat a `needs_review` correction as accepted. Identities are written only after manual
confirmation; the PR review records how they were checked.

The glossary input follows the [translation contract](../docs/schema/translation-contract.md)
and [glossary adoption rules](../docs/schema/glossary-adoption.md). The index closes the input
directory; keep documentation, candidates and audit reports outside `translations/`.
Concept keys are permanent. Edit translation YAML directly and review it in a PR; Git keeps the
history (ADR-0018). Official wording stays out of git. Mark quality with `low_confidence`
and `origin`; a `source_claim` is optional and only holds a work, URL or claimed source.

Class and card-type labels use catalog vocabulary references `(kind, code)`, rather than
glossary identities. Equal display text does not merge those references: resource EP/SEP
and their card types, or a class and a trait, remain distinct. Their translated labels
live in the catalog vocabulary record's `value.translations`.
