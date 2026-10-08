# Digital-link intake

This module implements the link entry of the digital-link adoption contract.
Coverage intake is not implemented; no record means unknown. Candidate reports
never adopt a relationship or manufacture a human receipt.

Load the entry with `load_links(authored_root)`. Each record is the current
relation for one subject: `subject`, `value` (relation, effect similarity and the
SVE/digital name references), `review_level` (`sampled` for a batch the maintainer
spot-checked, `confirmed` for an individually checked link) and a nonblank
`reason`. Git keeps earlier versions;
there are no adoption numbers, predecessors or batch decisions. `Inputs` reads the current working tree once and
`Inputs.configuration()` supplies descriptive `digital_link_authored` settings.

Build configuration also declares canonical-sorted unique `digital_link_sources`
(store/batch pairs), `catalog_registry`, `translation_recipes`, and
`digital_evidence` from `translations.digital.configuration()`.
Within a caller-owned transaction call `populate_links()`. It checks every record
once against the current build: the card must be known and published, the record's
own SVE and digital name references must resolve exactly, and its digital names
must still equal the current catalogue. A changed or removed digital name, a
retired card or a card outside publication makes the relation stale; stale
relations are reported and not materialized. Each materialized relation gets one
record-level `decision` row with its `review_level` as state, as the build DB
requires, linked to its authored shard.

The composer in `workflows/offline_names.py` passes the result as `links=result` to
`domains.digital.name_policies.application.prepare()` and `populate()`, which delegate to
`domains.digital.name_policies.projection`. Standalone `import_links()` owns its transaction. The
application consumes each owner's link proof through
`DigitalLinkResult.eligible_owner(db, sources, owner, name_ref=...)`, independently
revalidating its Japanese name, card, face, printing source and exact linked
digital name. Missing coverage does not prevent an eligible name.

`translations.names.resolve.prepare()` validates current concept associations
and semantic assignments. Its resolver reports `ambiguous_name_concept` or
`missing_name_concept` rather than choosing an arbitrary glossary concept; other
legal policy or counterpart candidates still require their own evidence.
`translations.counterparts.first_counterpart()` selects sv1 before svwb and rejects
different adopted names within the same game. The current application supports
known printed owners and generates current translations, uses and display
bindings. Complete builds retain actual stage usages in the input summary;
source and owner checks happen while preparing and applying their plans.

## Offline candidates

```bash
sve-carddb digital-links candidates \
  --draft /absolute/private/research.json \
  --context /absolute/private/context.json \
  --repository /absolute/repository \
  --store declared-store=/absolute/archive \
  --output /absolute/private/candidates.json
```

The context is a BuildContext JSON with the above frozen batches, registry and
recipes. Store options may repeat with distinct IDs. Output must be absolute,
not symlinked and disjoint from protected inputs. Only a completed report is
atomically replaced; no live manifest or source network is used.

Full frozen catalogues establish Japanese-name/Traditional-Chinese-name uniqueness.
SV1 requires unfiltered cards URLs; SVWB requires include_token=1 and complete
count/offset/card_details pagination. Class comparisons use the maintainer's
explicit enum table. Card-type triage currently uses a fixed base-label lookup
after separating the composite label; it is a mechanical candidate heuristic,
not adopted vocabulary or permission to import a relationship. Formal vocabulary
adoption remains the authority for production classification.

Reports contain IDs, references/hashes, row numbers and overlapping failure counts,
without card names or wording. Per-game tier counts inherit whole-row failures;
local counts describe that game's failures. Source occurrences count draft links,
not adopted links. Phase candidates still require human assignment. A candidate
never becomes a `same_card` record automatically; the maintainer writes the
record after checking it.
