# Digital-link intake

This module implements the link entry of the digital-link adoption contract.
Coverage intake is not implemented; no record means unknown. Candidate reports
never adopt a relationship or manufacture a human receipt.

Load the entire immutable authored entry with `load_links(authored_root)`.
`Inputs` compares exact index/shard bytes with the declared full authored Git SHA,
and `Inputs.configuration()` supplies the `digital_link_authored` build pin.

Build configuration also declares canonical-sorted unique `digital_link_sources`
(store/batch pairs), `catalog_registry`, `translation_recipes`, and
`digital_evidence` from `translations.digital.configuration()`.
Within a caller-owned transaction call `populate_links()`, then pass its result
to `populate_name_translation(..., links=result)`. Standalone `import_links()`
owns its transaction. Results retain fresh, stale and withdrawn terminal records,
decisions and the input usage record. Each name owner independently revalidates
its Japanese name, card, face, printing source and exact adopted digital name.
Missing coverage does not prevent an eligible name. Printed owners and semantic
context assignments remain separate work.

Historical review contexts validate their declared dependencies and recipes
against immutable Git blobs. They describe the past review, rather than requiring
the current runtime to equal the old one. Current code revalidates frozen evidence;
the current build separately pins and verifies the loaded runtime. Changed current
digital names make a relation stale; a program version change alone does not.
Complete builds must combine all stage usages and call
`input_record(...).verify(..., complete=True)` before export.

Receipts require an explicit nonblank author and the repository-listed maintainer
`gbaian10` as exact reviewer, as in the existing catalog adoption loader.
Synthetic tests exercise this format without creating real adoption data.

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
not adopted links. Phase candidates still require human assignment.
The canonical result hash excludes result_hash/adoption_background; the resulting
background binds recipe, draft, result and class-table hashes. Actual sampled or
confirmed receipts require the maintainer's real review event.
