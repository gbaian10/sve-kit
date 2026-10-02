# Template parameter candidates

This offline tool proposes `parameter_schema` and `source_span` for the sealed
current Japanese ability fields inventoried by `template_sources`. It does not
adopt templates, allocate permanent IDs, write a database, convert translations,
or participate in a build or preview. The authority for the shapes and meaning
of these fields remains [translation-contract §4](../../../../docs/schema/translation-contract.md).

```bash
uv --directory carddb run python -m sve_carddb.template_parameters \
  --store /read-only/archive --store-id explicit-store-id \
  --batch-id sha256:FULL_BATCH_HASH \
  --repository /checkout --code-revision FULL_GIT_SHA \
  --authored /checkout/authored --legacy /private/templates.jsonl \
  --vocabulary-proposals /private/bindings.json \
  --vocabulary-basis /private/confirmation-basis.md \
  --output /private/new-result-directory
```

All inputs are explicit; the command has no live manifest, network transport or
latest-cache fallback. `bindings.json` has the existing `Vocabulary` shape
(`bindings`, each with `region/kind/raw/code/special_kinds`). Its exact bytes and
the caller's confirmation basis are pinned separately. The basis is provenance
for a **proposal**, not a parsed adoption receipt. The glossary loader validates
the complete adopted closure and its Japanese spellings are independently
replayed through the existing frozen translation evidence reader. Missing archive
evidence fails the run rather than inventing a spelling.

The parameter recipe `template-parameters-jp-candidate-v1` pins the complete
first-party runtime and dependency lock through the first-checkpoint recipes,
plus exact glossary and vocabulary input hashes. The runtime must match the
requested Git revision. Output contains hashes, ranges, schemas, integer values,
concept IDs and fixed reasons; it contains no official text or names. Hashes use
the existing canonical-json-v1 and SHA-256 boundary. Candidate payload hashes use
the contract's six fields; a hash is not a permanent template allocation.

Source binding ordinals follow the first raw Unicode position. Multi-range body
spans exclude inline reminders, which anchor to that line's body ordinal.
Standalone reminders are unanchored. Each exact field, including its CRLF/LF and
edge whitespace, must independently roundtrip every UTF-8 byte. Unknown fields
are not converted to empty fields. Whole-prefix NFKC provenance tracks composition,
expansion and reordering as many-to-many raw ranges. Quoted and digit replacements
are separate from literal N/X. Fixed-text traces record literal positions without
turning foreign sentences into literal parameters.

Body, reminder and header retain the first inventory's exact normalized bytes.
Only layout has a new fixed candidate payload (`W`) and one exact-source-whitespace
literal parameter. `normalized_hash` remains the old inventory hash;
`template_normalized_hash` and `parameter_normalizer_id` describe the proposed
payload. Thus all layout spellings share one candidate while source bytes remain
independently pinned. This does not rewrite the legacy inventory.

Numeric candidates require exact ASCII/fullwidth decimal raw spellings within
JavaScript's safe unsigned range. Token cost/attack/health roles come from the
complete named header grammar. Other numbers require a bounded unit or prefix
grammar; signs, ASCII identifiers, compatibility numerals and unclassified bare
numbers remain unresolved. This conservative classifier is a proposal, not a
maintainer-approved semantic rule. Each occurrence has its own slot; grouping
multiple occurrences additionally requires equal values.

Quoted references can resolve to a unique exact adopted Japanese `card_name`
concept (`term`). No trim, NFKC, case folding or name-to-card lookup supplies an
ID. A card identity adapter and the `card_name_concept` exception loader do not
exist at this checkpoint, so the tool never proposes a guessed `card` parameter;
explicit back-face references must also wait for adopted concept evidence.
Braced adopted terms are candidates pending semantic role review; ordinary
glossary substring mentions are separate diagnostics and never bindings.
Header/braced class/type references use explicit proposed vocabulary mappings,
preserve special flags, and remain unresolved until an adopted catalog adapter
exists. Composite types require separate slots rather than discarding flags.
Mechanical fullwidth-parenthesis extraction remains pending classification review.

`candidates.jsonl` lists every source entry, including null schemas and individual
slot reasons. `legacy-lineage.jsonl` lists every old parent and its source members.
Mechanical provenance forks (including literal-N collisions) and pending semantic
review are counted separately. All `supersedes_id` values are null: no old parent
has an approved payload here, and no fake parent row or permanent child ID is made.
`term-mentions.jsonl` and `field-spans.jsonl` hold the other diagnostics.
`sources/` retains the existing bounded YAML source inventories and coverage proofs.
Output is published atomically into a fresh directory outside immutable inputs.

Fingerprint reproduction, legacy member coverage, full source coverage and
parameter completeness are separate gates. Exit 0 requires all gates, exit 1
publishes honest unresolved candidates, and exit 2 reports a safe input/evidence
failure without printing source text. A complete candidate schema is not adoption.
The source coverage check still uses presence v1; consuming presence v2 requires
a preceding docs PR updating authored-layout §9.8. This tool does not wire it in.
