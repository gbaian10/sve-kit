# Template definition and translation intake

`loader.load_templates(repository, authored_revision, sources)` validates the
complete immutable `authored/translations/index.yaml` closure before exposing any
adopted definition or terminal translation revision. Its wire models follow
`docs/schema/translation-contract.md`: definitions have exactly nine data fields;
their content hashes cover the six-field semantic payload reconstructed from the
source, while record membership includes evidence. Unknown fields and unsupported
record kinds fail closed. Glossary shards use their existing complete validator.

`files` reads exact Git bytes, verifies canonical YAML hashes, ordinary file modes,
indexed file closure and contiguous shard numbers. It checks the complete ancestor
history for modification or deletion of published template shards and inventories,
including changes later reverted. A shallow checkout cannot validate that history.
Dirty worktree files do not alter pinned inputs.

`sources.TemplateSources` requires a complete frozen JP batch, its extractor,
classification and parameter recipes, the exact legacy input hash, and reference
and recognition-policy pins. It reuses the registered normalizer and approved
recognition replay; callers cannot supply normalized strings or resolution claims.
The formal inventory must contain every reconstructed entry and match every field.
Source-coverage and fingerprint reports remain separate: known, individually
verified fields may be adopted while the presence-v1 batch coverage remains false.

Definitions must cover every parameter position with the recognized type, role,
bounds and exact raw spelling. A merged slot requires both equal values and equal
roles. Unknown card-name and unimplemented vocabulary references remain pending.
Keeping a legacy ID requires agreement across its entire normalized family;
otherwise only compatible members count toward a new payload ID. Supersedes
requires an adopted complete parent and forbids cycles. Frequencies count verified
source entries once and sort by descending count, then ID.

The initial loader accepts human `sampled` or `confirmed` batches, preserves their
actual checked subsets, verifies final model-review hashes, and rejects translation
revision gaps and duplicate records. Disputes require that particular member's
actual human sample and matching resolution event. `approved_policy` has a closed
wire shape but is deliberately refused until the separate translation-policy loader
and actual initial-sample prerequisites exist. Recognition approval cannot replace
definition or translation adoption. This first implementation supports JP sentence
definitions with the default semantic variant and the four implemented source roles
(body, reminder, token_header, layout); other languages, variants and roles fail.

`text.parse()` implements the finite translation language from
`docs/schema/translation-contract.md` §4.2. A parameter is exactly
`{{slot_name}}`; literal braces and backslashes must be escaped. It rejects
unknown or unused slots, malformed braces, invalid escapes and expressions.
Source and target positions use Unicode code points, including astral characters.

`preparation.convert()` rewrites a legacy draft using an explicit target alignment.
The alignment pins both the exact draft text hash and the canonical parameter
schema hash. Sorted, disjoint target spans must match their raw hashes and cover
each declared source occurrence exactly once. This conservative migration helper
requires occurrence preservation; the general text parser only requires each slot
to be used. Target slots may reorder the source slots. Literal `N`/`X` characters
outside the supplied spans remain literal. No ordinal pairing or global letter
replacement establishes a semantic correspondence.

The resulting `Prepared` value always remains `candidate_only` with machine origin.
It keeps the draft's confidence for private preparation and never contains a
decision, membership, policy adoption or model review. These candidate fields
are not the authored `template_translation.data` wire format. Do not put candidates
or legacy drafts into `authored/translations` or a publication database.

`review.verify()` checks a separately recorded review against the final exact text
hash. Changing placeholders, escaping or wording requires another review. The
closed model records both models and versions and distinguishes agreement from an
unresolved dispute; a legacy bare `ok` verdict is not this review. The dispute
guard requires both a recorded human resolution and an actual sample declaration.
It does not validate the declaration's provenance or create a human event.

The loader validates recorded decisions; it never creates human events or receipts.
Actual definition and translation sampling, translation-policy loading, database
import, binding and rendering remain separate work. No build or preview entry is
wired to this module. Keep original/final translation bytes and alignment review
evidence private until a valid authored batch can be produced.
