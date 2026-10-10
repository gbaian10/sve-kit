# authored

Human-maintained data and permanent identity allocations, read by `carddb` at build time.

- `registry/card/`, `registry/face/`, `registry/printing/` — permanent identities and manually confirmed JP/EN membership, filed by immutable owner
- `ids/index.yaml`, `ids/<owner>/*.yaml` — allocation policy and cursors, and append-only printing `int_id` allocations in per-region ranges
- `registry/region-mapping-review/` — confirmed absence of Japanese counterparts, with scope and date
- `registry/art/` — reviewed English original-art uses
- `registry/card-related/` — source-pinned reskin relationships
- `registry/source-correction/` — confirmed corrections and separately marked review candidates; raw observations remain unchanged
- `rules/effects/` — effect data in the DSL defined by `../dsl/`
- `rules/rulings/` — evidence-backed interpretations; see `../docs/adr/0009-rulings-evidence.md`
- `translations/glossary/<filing_key>/*.yaml` — current glossary concepts and editable current translations
- `translations/flavor/<hash digit>.yaml` — whole-paragraph flavor translations keyed by the source text's SHA-256; see [flavor translation](../docs/schema/domains/flavor-translation.md)

Every YAML document explicitly declares integer `format` and enumerated `kind`.
Templates use `translations/templates/{definitions,values,candidates}/`; products
use `products/{family,product,inclusion,identities}/`; catalog data lives under
`catalog/{adoptions,overrides}/`, and digital policies are
`digital/policies/{names,links}.yaml`. Rules also include `rules/keywords.yaml`
and `rules/engine/index.yaml`; source-bound crops are `images/crops.yaml`.

The identity registry format is defined in [authored layout](../docs/schema/domains/authored-layout.md).
The former `card-ids.yaml` proposal is replaced by `registry/` and `ids/`.
Run the offline generator with `python -m sve_carddb.domains.registry --help` through the carddb uv environment.
Never regenerate identity IDs from sorting, rewrite an existing registry record in place,
or treat a `needs_review` correction as accepted. Identities are written only after manual
confirmation; the PR review records how they were checked.

The #474 evidence representation migration and #476 document envelope migration
are one-time exceptions to the prohibition on rewriting records. Each requires
a fixed baseline, an isolated conversion using the existing encoder, a complete
diff and review before an atomic switch of readers and data. Permanent IDs,
allocations and manual membership remain fixed. The [migration boundary](../docs/schema/domains/authored-layout.md#2-分片與來源)
does not relax the daily append-only tools or add a general rewrite command.

The glossary input follows the [translation contract](../docs/schema/domains/translation-contract.md)
and [glossary adoption rules](../docs/schema/domains/glossary-adoption.md). Readers load the dedicated working-tree data directories once; keep documentation and audit reports outside `translations/`.
The controlled `translations/templates/candidates/` area is the exception for
validated template candidates; candidates never supply formal template translations.
Concept keys are permanent. Edit translation YAML directly and review it in a PR; Git keeps the
history (ADR-0014). Official wording stays out of git. Mark quality with `low_confidence`
and `origin`; a `source_claim` is optional and only holds a work, URL or claimed source.

Class and card-type labels use catalog vocabulary references `(kind, code)`, rather than
glossary identities. Equal display text does not merge those references: resource EP/SEP
and their card types, or a class and a trait, remain distinct. Their translated labels
live in the catalog vocabulary record's `value.translations`.
