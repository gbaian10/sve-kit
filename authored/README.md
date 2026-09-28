# authored

Human-maintained data and permanent identity allocations, read by `carddb` at build time.

- `registry/card/`, `registry/face/`, `registry/printing/` — permanent identities and manually confirmed JP/EN membership, filed by immutable owner
- `ids/index.yaml`, `ids/<owner>/*.yaml` — checksummed registry index and append-only printing `int_id` allocations in per-region ranges
- `registry/region_mapping_review/` — confirmed absence of Japanese counterparts, with scope and date
- `registry/art/` — reviewed English original-art uses
- `registry/card_related/` — source-pinned reskin relationships
- `registry/source_correction/` — confirmed corrections and separately marked review candidates; raw observations remain unchanged
- `effects/` — effect data in the DSL defined by `../dsl/`
- `rulings/` — evidence-backed interpretations; see `../docs/adr/0011-rulings-evidence.md`
- translations and glossary (zh-Hant) — layout remains proposed

The identity registry format is defined in [authored layout](../docs/schema/authored-layout.md).
The former `card-ids.yaml` proposal is replaced by `registry/` and `ids/`.
Run the offline generator with `python -m sve_carddb.registry --help` through the carddb uv environment.
Never regenerate IDs from sorting, edit an adopted batch in place, or treat a `needs_review` correction as accepted.
