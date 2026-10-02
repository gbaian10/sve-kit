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
complete named header grammar. Header traits reuse the existing JP trait parser,
including its embedded-separator exceptions, and require a unique exact adopted
trait concept. Unknown trait layouts remain unresolved. Other numbers require a bounded unit or prefix
grammar; signs, ASCII identifiers, compatibility numerals and unclassified bare
numbers remain unresolved. This conservative classifier is a proposal, not a
maintainer-approved semantic rule. Each such numeric occurrence records a closed
ASCII `numeric_rule` ID and `numeric_rule_pending_approval`; its schema and payload
hash stay null until a future approval-aware workflow supplies approved evidence.
Header numeric roles and unresolved numbers have `numeric_rule=null`. Signed and
identifier exclusions precede rule matching; the suffix takes precedence if both
suffix and prefix match. The pinned classifier defines these proposed rules:

| Rule ID | Exact context after/before the numeric occurrence |
| --- | --- |
| `suffix_unit_cards` | after: 枚, not followed by 目 |
| `suffix_unit_entities` | after: 体, not followed by 目 |
| `suffix_unit_points` | after: 点, not followed by 目 |
| `suffix_unit_times` | after: 回, not followed by 復 or 目 |
| `suffix_unit_turns` | after: ターン, not followed by 目 |
| `suffix_unit_pp` | after: PP, not followed by 目 |
| `prefix_field_cost` | before: コスト |
| `prefix_field_attack` | before: 攻撃力 |
| `prefix_field_health` | before: 体力 |
| `prefix_field_pp` | before: PP |
| `prefix_field_level` | before: レベル |

A suffix must be immediately adjacent and must not be followed by an ASCII
letter, digit or underscore. A prefix must be immediately adjacent, optionally
separated from the number by one `=`, `:` or `：`. Context is the normalized body
or the unchanged reminder. The IDs are provenance for proposals, not approval
receipts. `numeric_rule_counts` counts positions, including zero-count rules;
`unresolved_reasons` counts affected candidates once per reason. For each role,
`complete_without_numeric_rule_approval` counts issue-free schemas and
`complete_after_numeric_rule_approval` counts candidates whose **only** remaining
reason is numeric rule approval. The latter do not count as `complete_schemas`.
These counts do not constitute template adoption. Candidate uint bounds are
0 through the JavaScript safe integer maximum. Future ordinal schemas use a
lower bound of 1. Bounds belong to each template's schema/payload hash, not a
recognition policy, and observed maxima are not future semantic ceilings.

The recipe separately pins `numeric_classifier.version=numeric-rule-proposals-v3`
and its diagnostic definitions. This does not change the legacy text normalizer,
source partitions, parameter text-normalizer ID, or old fingerprints. Each recipe
still pins its exact Git revision and full runtime; older evidence is never
reinterpreted with the new classifier. After sign and ASCII-identifier guards,
recovery and ordinal lexical vetoes precede both suffix and prefix matching.
Excluded positions remain uint hints with exact raw values/spans, a null active
rule, and `numeric_recovery_amount_requires_review` or
`numeric_ordinal_requires_review`. A matching prefix cannot assign them an
incorrect fallback role. They never count as conditionally complete schemas.

`inactive_numeric_rule_candidates` lists two **disabled** proposals, including
their exact ASCII-escaped suffix patterns, position/member counts and locators:

| Proposal ID | Immediate normalized context after the number | Status |
| --- | --- | --- |
| `candidate_recovery_amount` | 回復 | proposal only; recovery quantity requires review |
| `candidate_ordinal` | one of 枚/体/点/回/ターン/PP followed by 目 | proposal only; ordinal role and lower bound require review |

These lists include only positions passing the preceding sign/identifier guards.
Exact raw decimal/safe-value checks remain independent; a lexical diagnostic does
not authorize an invalid value. Body normalization and unchanged-reminder rules
remain distinct. Neither proposal is an active `numeric_rule` value, nor do these
diagnostics supply a schema, payload, approval policy or adoption receipt. Other
quantity contexts (including per-turn frequency, separated card quantities and
unclassified `つ` quantities) are unchanged pending their existing semantic review.

## Opt-in recognition proposals

Repeat `--enable-candidate-rule RULE_ID` to emit a pending recognition proposal
from the closed `parameter-rule-candidates-v1` registry. All 17 rules default off.
Unknown and duplicate IDs fail; selecting a family is not an implicit switch.
The recipe pins every condition and its canonical hash, the exact enabled set,
and `recognition_policy=null`. No policy loader or adoption authority is supplied.
Eight earlier numeric grammars have a maintainer statement; this command does
not interpret that statement as an approval receipt or remove their pending flag.

`rule-candidates.jsonl` records each match's rule ID, condition hash, proposed
role, original reason, raw hash, exact source segments, normalized occurrence,
value or adopted concept hash, and any full-field context proof. Its status is
always `pending_approval`. The original candidate issues, schema, payload hash,
literal trace and `supersedes_id` remain unchanged. `candidate_rule_counts` counts
positions and distinct uses separately for the whole batch and body role.

| Family | Candidate rule IDs |
| --- | --- |
| Damage quantity | `suffix_damage_amount` |
| Cost magnitude with literal sign | `prefix_cost_delta` |
| Recovery and ordinals | `suffix_recovery_amount`, `suffix_ordinal_cards`, `suffix_ordinal_times`, `suffix_ordinal_turns` |
| Closed ability thresholds | `keyword_threshold_combo`, `keyword_threshold_lesson`, `keyword_threshold_necrocharge`, `keyword_threshold_spell_chain` |
| Generic items | `suffix_unit_items` |
| Choice option indices | `bracket_choice_index` |
| Closed braced references | `braced_stat_reference`, `braced_ability_reference`, `braced_action_engage_reference` |
| Stat magnitudes with literal sign | `prefix_attack_delta`, `prefix_health_delta` |

Precise regexes, exclusions, concept sets and non-regex evidence requirements are
in `rule_candidates.definition`; synthetic positive/negative cases are in
`test_template_rule_candidates.py`. Every new match must concern an originally
unowned pending slot. New rules never take over any existing `numeric_rule`.
On upgrades, manually replay the frozen batch and compare every old position's
inventory/slot/span/value/raw hash/rule, not only the 14,782-position count.
Upgrading glossary pins must run the same comparison: a newly ambiguous name
safely stops matching and requires review rather than choosing one concept.

Sign exclusion now also handles fullwidth plus/minus in unchanged reminders.
This conservative change affects zero earlier owned positions in the reviewed
JP batch. It must be disclosed with the earlier `entities` description mistake:
that grammar includes only `体`, while `つ` is a new, unapproved candidate.
Unicode minus and unchanged-reminder fullwidth signs receive no signed proposal;
signed grammars support only ASCII plus/minus after the existing body normalizer.
Optional separators and zero-observation ordinal units are not added. Signs stay
literal, while only unsigned magnitudes receive slots.

Braced reference slots cover the original exact name, not a substituted
placeholder; their fixed per-template value leaves old fingerprints unchanged.
The recipe already pins glossary revision and index hash in `references`.
Only the closed 11 adopted IDs are recognized, with exact unique raw spelling,
record category and record hash. This recognizes a word, not its translation or
effect. An ability threshold similarly verifies its exact adopted name as
context, but that name remains literal; only its number is the threshold slot.
Unadopted names and `earth_rite` remain pending.

Choice rules need an earlier finite choice introduction in the same full field,
and a complete contiguous 1..k label group with at least two options. Introduction
and all labels must belong to body spans; reminders, other fields, reordered,
missing or duplicate labels cannot supply context. Their exact context spans and
hash travel with each proposal, including when the introduction is on a prior
line. This is evidence-dependent matching, not a global property of a legacy ID.

The candidate tool and private confirmation-page data are separate from future
docs, policy loader and authored policy PRs. A confirmation page must bind each
rule's condition hash to the eventual merged matcher commit before review.
No live source, receipt, translation or build/preview integration is introduced.

Each occurrence has its own slot; grouping
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
exists. The formal catalog derivation entry added by #209 should replace the
private proposal/basis inputs when formal vocabulary adoption records are
available. Only replayed adopted catalog evidence may remove the pending reason;
this checkpoint does not wire that future adapter in. Composite types require
separate slots rather than discarding flags.
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
