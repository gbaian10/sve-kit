# Template definition and translation intake

`loader.load_templates(repository, authored_revision, sources)` validates the
complete immutable `authored/translations/index.yaml` closure before exposing any
adopted definition or terminal translation revision. Its wire models follow
`docs/schema/translation-contract.md`: definitions have exactly nine data fields;
their content hashes cover the six-field semantic payload reconstructed from the
source, while record membership includes evidence. Unknown fields and unsupported
record kinds fail closed. Glossary and name-override shards use their existing
complete validator. Both area loaders verify the entire index and hash closure;
the glossary projection validates foreign template envelopes and inventories, while
source replay and semantic definition checks remain the template loader's responsibility. Glossary pins retain every closure file, including inventories.

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
otherwise only compatible members count toward a new payload ID. A content hash
has exactly one allocated ID, and an ID longer than 16 hex requires a different
adopted payload at every shorter two-hex prefix. Complete hash collisions compare
bytes and fail. After superseded definitions are retired, a source member can match
at most one current definition; current frequencies count each entry once.

Supersedes accepts an adopted parent from the same verified source family, or that
family's unadopted legacy ID, and forbids cycles. The latter stays in immutable
provenance and is exposed in `Snapshot.unadopted_parents`; `database_parent()`
returns null until there is an adopted parent payload, without creating a parent
row. Definition evidence must locate a member of its verified family.

The initial loader accepts human `sampled` or `confirmed` batches, preserves their
actual checked subsets, verifies final model-review hashes, and rejects translation
revision gaps and duplicate records. Disputes require that particular member's
actual human `sampled` declaration and matching resolution event. A `confirmed`
batch currently does not resolve a machine dispute; use the explicit sampled
event. Translations in the source language are refused. `approved_policy` has a closed
wire shape but is deliberately refused until the separate translation-policy loader
and actual initial-sample prerequisites exist. Recognition approval cannot replace
definition or translation adoption. This first implementation supports JP sentence
definitions with the default semantic variant and the implemented effect roles
(body, reminder, token_header, layout), plus the independent exact flavor path
below; other languages, variants and roles fail.

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

The immutable-history guard currently conservatively requires one publication
sequence across the traversed history. A general merge with an older divergent
side branch can be refused even if its merge result is valid. Current squash
merges avoid that case; supporting branch-local immutable histories is separate
work. The published index-entry guard is a second layer: a repointed hash is
already caught by the indexed file hash before that guard can run.

## Historical inventory v2 structure

`replay_models` defines the closed inventory v2, semantic manifests, producer
environment, per-inventory effect/flavor inputs and six-stream hash-only expected
output manifest. Its root hashes the format, recipe and complete stream digests;
the root itself is excluded. Flavor batches and identity basis come from each
context, rather than being inferred from the current caller's basis. Recipe
and entry fields, translation records and adoption requirements are unchanged.
`InventoryV2.group_key()` includes both recipes and the complete replay context;
different producer environments, inputs, bindings or expected outputs cannot
silently share a cache identity.

The glossary loader validates foreign envelopes and known semantic versions
without recursively replaying their sources. The template loader rereads every
immutable inventory, groups its complete recipe/context, then uses the installed
finite dispatcher to reconstruct the full batch and compare all six expected
streams. A correct declared root alone is structural evidence, not proof that any
source was read or a definition adopted. Legacy v1 retains its original replay
checks; no v1 inventory is silently upgraded.

The implementation contract is `docs/schema/template-source-replay.md` (C+hash).
Environment values are provenance, not a host-equality gate. The implementation
verifies exact producer/installed semantic files, recipe pins and necessary
package evidence, compares complete outputs, and bounds replay with nested hard
budgets. Its independent source plan and single executing-host F1 bundle are
described in `../template_semantics/README.md`. Source generation only writes
private candidates. No actual inventory may be adopted before this capability
merges, a candidate is generated with the merged producer and independently
replayed, and the required engineering/adoption reviews complete. Application
build/preview and authored adoption remain separate work.

## Exact flavor replay (legacy v1)

`flavor_pins.recipes(repository, revision)` emits `flavor-exact-v1` with exactly
empty config and the separately pinned JP parser. Replay verifies the complete
installed first-party module set, exact Git bytes and dependency lock, refusing
changed, extra or missing modules. It does not run the effect normalizer or borrow
recognition consent. The frozen effect matcher files and recipe remain unchanged.

`TemplateSources(..., flavor=FlavorInputs(source_batch=..., identity_basis=...,
identity_batches=...))` pins one complete JP card batch separately from recipe
options. Identity batches explicitly cover every base printing observation in the
pinned registry, including EN identity claims when present; the flavor inventory
itself still enumerates only the chosen JP batch's current physical face fields.
The runtime dependencies, source batch, identity revision/index, exact identity
files and observation uses are retained in `source_reports`. Missing registry
owners remain pending; a source-only preview with `identity_basis=null` cannot
adopt a definition. Invalid observations or identity pins fail rather than becoming
missing translations. No caller-provided normalized string or owner resolves a
pending entry.

The independent closed `FlavorEntry` and `FlavorSpan` union members preserve the
existing eight- and nine-field wire formats. Each present field yields one whole
code-point span, sentence level, zero slots and no legacy fingerprint. Unknown,
exact empty and fixed-White_Space-only fields remain separate report states;
whitespace-only fields never acquire a template or a missing-translation claim.
Original punctuation, numbers, names, newlines and spaces remain exact. The
parser's frozen full-field projection and the exact normalizer are different pins.

`FlavorOwners` reuses immutable registry and full frozen observation replay and
maps each source to its own confirmed printing/face. Shared paragraphs can share a
payload but retain separate physical owners. `flavor_owners.verify_owner()` is a
read-only downstream guard: the printing face's own `flavor_unit_id` must match
both the context source and exact text-unit hash/language. It neither reads the
current effect nor drops a known flavor because effect wording is pending. It does
not insert a context, use, selection or translation, or provide rendering/build
wiring.

Flavor definitions require `ja`, `default`, exact recipe, a whole-field span,
zero parameters and a new payload ID. Final translations validate both the formal
escaped bytes and decoded display: LF only, no outer whitespace or trailing
whitespace on a nonempty line. Invalid final bytes require correction and another
real model review; the loader never normalizes them. Human adoption remains the
only supported path. Flavor policy adoption and actual initial human samples are
separate prerequisites, not granted by a candidate page or an agreed model review.
