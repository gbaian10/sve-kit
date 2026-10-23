# Effect DSL

[effects.schema.json](effects.schema.json) is the sole grammar authority for
`astra/1`. Documents use `format: 1, kind: effect_set`; keyword registries
use `format: 1, kind: keyword_registry`. These envelopes select the existing
prototype grammar and keep Schema `$id: urn:sve-kit:effects:astra:1`.
The [author 1.0 contract](../docs/dsl/author-contract-1.0.md) specifies separate
`author_effect_set` and `effect_body_shard` entries. Their closed schemas and
Python structural readers and explicit expansion APIs are available; source admission remains separate. The prototype's 73 node variants contain 47 action atoms, 10 combinators,
14 declarations and two loading/unsupported markers.

The Rust prototype validates `authored/rules/keywords.yaml` and every pack file, expands
macros, then validates the expanded programs again. `carddb` does not yet invoke
this schema and Rust types are not generated from it.

`astra/1` is the D-stage prototype grammar: its semantics are defined by the engine
and its tests, and [docs/sim/engine-status.md](../docs/sim/engine-status.md) lists
the executable subset. The Effect DSL 1.0 specification that replaces it is in
[docs/dsl/](../docs/dsl/README.md). Schema validation does not establish card-text
correctness or runtime support.

`optional.selection` and resolution-time `choice.selection` explicitly combine
one selection with the execution or mode decision. Separate `select` nodes
remain separate input points. Declining clears the selection binding; an
explicit zero-card execution is a distinct option when `min` permits it.

Prototype effect `review` notes are optional and do not authorize execution or
represent an acceptance state. Keep specific explanations; omit boilerplate.

The [construct inventory](../docs/dsl/author-construct-inventory-1.0.md) maps
author syntax §§2–9, all 63 GAP-A families and the 22 supplementary sections.
It includes prose-only shapes and canonical IDs; it is not an executable grammar.

[author-nodes.schema.json](author-nodes.schema.json) defines the closed author
leaf parameters (`A.param`, declarations and eight typed value variants) and
reference, player, leader-recipient and duration atoms. Their required fields,
exclusive variants and roles are listed in the
[reference shape matrix](../docs/dsl/author-reference-shapes-1.0.md).
Its root rejects standalone documents; select a named `$defs` entry.
A reference atom is not a complete action-position type: positions such as
`move.ref` also accept selectors; the closed action-position types are documented in the action and step matrix.
Independent `first`/`second`/`rest` views are rejected. Use target roles or bindings, grouped search destinations, and successful-before set differences.
The leader union shares `A.single-leader`, `A.multiple-leaders` and `A.leader-of`
without duplicate branches. The [selection shape matrix](../docs/dsl/author-selection-shapes-1.0.md)
defines Target role declarations and the named `RecipientSel` sum, and distinguishes quantity
rules for each position. The [quantity matrix](../docs/dsl/author-quantity-shapes-1.0.md)
defines closed single-group Target/choose/search quantity variants and counter costs.
Their source and binding/destination fields are present. The
[cost and target matrix](../docs/dsl/author-cost-target-shapes-1.0.md) adds ordered Cost
steps, payment wrappers, material variants, X declarations and grouped Targets.
The [action and step matrix](../docs/dsl/author-step-shapes-1.0.md) adds choose/search groups, destinations, actions, and recursive Steps. It keeps pre-payment decisions separate from payment receipts. Counter holder quantities are separate
from counter amounts, and only selection n/upto accept direct selection parameters.
DeckSlice accepts a player or player set and shares the same nonnegative-literal
Quantity as cost bounds and counter quantities. NumericExpr and Quantity reference
one shared NumericOperation union. CounterName has no parameter permission.
DeckSlice consumers are documented at action-level top; the complete move/banish/look/reveal
parents share the registered DeckSlice type; cost.banish.top uses it too. ObjectSel and Ref reject top/bottom aliases.
Selector-level slices remain registered in prose pending independent consumer evidence.
Event object references currently admit
only `holder`; other typed event projections are outside this subset.
The [numeric shape matrix](../docs/dsl/author-expression-shapes-1.0.md) covers
`A.expr.numeric`, `A.cond` and their variants: signed arithmetic,
object/player and aggregate numeric reads, predicates, and `cmp`/`all`/`any`/`not`. Every variant's operand
domain is also partial; these definitions are not complete Expr/Cond types.
The same recursive unions include `count_of` and `did`. Separate `objects_of`
snapshots require an explicit `before`/`after` view; `same_as` reads those snapshots.
Material predicates admit nonempty `name`, `zone`, `kind`, `trait`, `not_class`,
`orig_cost_eq` and `orig_cost_ge` fields, with Quantity comparisons and no direct selector parameter permissions. Receipt types,
units, available material projections and typed success predicates require lowering.
Only the registered plus/minus arithmetic operand positions accept parameters. Nested
arithmetic must still be checked against the full source operand path.
The [object query matrix](../docs/dsl/author-object-query-shapes-1.0.md) defines
zone and binding/snapshot object sources, object `any_of` unions, `except`, `where`/`as`, and a closed
attribute subset. `count` accepts zone or binding/snapshot sources, `except`, `where` and `as`.
CountAtom combines counted-owner/zone queries with CollectionIn; count unions deduplicate
membership. CollectionSource serves aggregate/distinct reads and count exclusions, without
the direct counted-owner/zone parameter permission. Nested counts retain their own roles. These queries have their own leaf parameter positions; role checks,
scope, identity and operation authority still require lowering. Object selections
exclude leader recipients. `RecipientSel` adds the existing leader atoms, strict
`leader_or` and unions containing at least one leader limb. The array-level
`contains` constraint separates those unions from object-only unions. Unions
deduplicate membership without creating object order; equivalent author spellings
do not imply identical body hashes. Consumer role and batch semantics still require
lowering. The [event and history matrix](../docs/dsl/author-history-shapes-1.0.md)
registers event/history narrowing, history_count/history_sum, event numeric reads,
LKI operands, capture wrappers, chosen, paid, x_paid, dice, PlayerExpr equality,
material filter extensions and agg.hp. The [per-event matrix](../docs/dsl/author-event-matrix-1.0.md)
separates required and optional payload fields from history.where for all 33 base
event keys, plus the counter_reached macro. The
[effect matrix](../docs/dsl/author-effect-shapes-1.0.md) adds 27 closed EffectSpec keys,
cannot variants and rule overrides. The [ability matrix](../docs/dsl/author-ability-shapes-1.0.md)
defines complete ability heads, the granted-head subset, conditional play modifiers,
and delayed triggers. `contracts/author_programs.py` owns their shared recursive
ability/effect/step graph. Grant payloads can combine keywords and abilities and
accept leader recipients; equipment and nested static grants use the same graph.
Effect parent defaults, source scopes and execution capabilities require C1/D0;
fragment acceptance is not enclosing-context validation. Full authored file/card
containers use the structural reader described below; explicit expansion validates
the same graph through `$defs/expanded`. Snapshot count/exists do not establish hash
equivalence with count_of/did. Rust decoding and execution remain separate.
[effects-index.schema.json](effects-index.schema.json) defines the complete
`format: 1, kind: effects_index` envelope and record syntax. Both use JSON Schema
2020-12. The `AuthorSchemas` reader in
`carddb/src/sve_carddb/domains/effects/author_inputs.py` takes the explicit schema
directory and validates leaf and index inputs with local resources only.
It does not expose a reader for the reference atoms, target roles or numeric fragments.
Python types in
`contracts/author_leaves.py`, `contracts/author_references.py` and
`contracts/author_queries.py`, plus target roles in `contracts/author_selections.py`, are checked
against every implemented definition by structural drift tests. Reference
acceptance tests also check reserved-name separation, required fields, exclusive
variants and nested rejection of parameter nodes.
Leader union checks flatten nested unions, reject duplicate branches and require
each accepted example to match exactly one leaf. Role declarations reject empty
maps, invalid names and booleans masquerading as the integer literal 1.
Numeric drift checks preserve named reference targets and compare every recursive
definition individually. Numeric acceptance tests check signed bounds, arity,
exclusive operators, parameter placement and wire-key round trips.
`contracts/author_receipts.py` retains the nonrecursive snapshot and same-as leaves.
Material filters, count_of, did, object and recipient shapes share the recursive
graph in `contracts/author_queries.py`. Omitted material fields remain absent and
explicit nulls are rejected. Drift and acceptance checks cover that graph,
including negative mutations of union membership and `contains`.
The drift checker rejects unsupported constraints beside `$ref` rather than
silently ignoring them. Index format literals reject booleans and floating points
in the Python contract as well as at the public reader boundary.

The reader rejects non-JSON YAML values, unsafe numbers, duplicate/non-string
keys, unknown fields, wrong envelope versions and duplicate index card/scope
pairs. Diagnostics expose structural paths and available YAML coordinates without
source excerpts. Parameter placement, source-role compatibility, candidate-file
closure and registry membership are separate checks, not implemented by this reader.
Source-role checks include the allowed type/component combinations in contract §3.
Concept values are nonempty author strings, including Japanese trait/class names;
they are not restricted to the ASCII `Code` syntax used for source-slot names.
For non-string keys and merged keys without direct cursors, coordinates identify
the containing mapping; ordinary property paths locate their value node.
Reference syntax does not validate binding scope, event-field availability,
singleton cardinality or permission to follow a movement receipt.
The complete-container APIs below add `author_effect_set`, `effect_body_shard` and
the recursive `authored` profile. Explicit expansion also uses the `expanded`
profile; structural acceptance is not runtime support.

## Complete author containers

[author.schema.json](author.schema.json) and [effect-bodies.schema.json](effect-bodies.schema.json)
select the shared `authored` and `A.body-shard` definitions. `AuthorSchemas.document`
dispatches by registered kind; `author_effect_set` and `effect_body_shard` decode
closed typed containers. `author_effect_set` and `document` accept an optional
validated `AuthorIndexRecord` context for diagnostics, including parser failures.
The [entry shape matrix](../docs/dsl/author-entry-shapes-1.0.md) describes all fields.

The reader checks strict YAML/JSON, local duplicate and evidence invariants, and
rejects typed `ParamRef` leaves anywhere in inline abilities. The shared schema
and decoder graph alone still accept these leaves; companion bodies need them.
`author_tree` and `body_tree` apply the same checks and return the original strict-loader
JSON tree, preserving optional fields instead of reconstructing hash payloads from
models. `document` reuses its parsed document when dispatching. Mechanical review
body references must match their use dependencies in frame, ability scope, body
version and applicability scope. Nonempty `verified_by_exam` remains unsupported.
These readers do not establish source admission, qualification or executable support.
No prototype data is migrated.

## Expansion candidates

`AuthorExpander` in `carddb/src/sve_carddb/domains/effects/author_expansion.py`
selects exact body versions and registrations, verifies `effect-body-v1` against
original JSON payloads, and compares full canonical payloads even when hashes match.
It replaces only typed parameter leaves and preserves array order, bindings and
arithmetic. Canonical ability comparison removes only top-level use locations
(`line`, `section`, `sources`); ports and imports remain significant.

| API | Result and evidence boundary |
| --- | --- |
| `AuthorExpander.expand` | `ExpansionCandidate.tree` plus a separate `sidecar`; each ability has `source_proof=none` |
| `AuthorExpander.expand_sources` | Reconstructs supplied source candidates and validates supported role/operand mappings; successful evidence has `source_proof=derived_self_consistent` |
| `load_author_set` | Reads every file in the fixed author/body/prototype areas and validates two-way index closure; companion versions and keys are checked across shards |
| `CurrentAuthorSources.validate(authored_root, expander)` | Loads the complete inventory from disk and returns one accepted candidate or rejection per indexed card/scope; accepted evidence has `source_proof=current_source_checked` |
| `CardAccepted.lowering_input()` → Rust `ExpandedLowerer.lower_current` | Internal handoff of the accepted tree, current-checked source sidecar, permanent CardId, current face/revision mapping and explicit token links; returns candidate IR, the expanded tree and a separate checked source sidecar. `CurrentCardProgram.source_at(author_path)` resolves expanded descendants and IR instruction paths to permanent owners, original coordinates and checked SourceUse evidence |
| `AuthorSchemas.expanded` | Validates the complete tree against `$defs/expanded` and independently rejects decoded `ParamRef` and `GeneratedBy` nodes |

No evidence state means qualified or executable. The current boundary loads the
complete inventory from the authored root itself; a caller-supplied or reduced
`AuthorInputSet` cannot grant current proof. It then verifies JP
current revisions, exact registry Frame/Binding matches, the complete
`effect-source-v1` fingerprint and supported entry payloads. All companion records
are checked against the registry, including unused bodies. Historical
qualification, complete use dependencies and engine exams remain separate.
File/schema/index failures reject the inventory; ordinary unsupported sources reject
only that card/scope. An EN inventory reader closes file/index references but always
rejects EN source admission. Unindexed prototypes are validated and checked for
shared-card overlap, without requiring their migration into the new index.

The current handoff resolves ability owners from their exact current revision,
including abilities in face containers. It does not infer identity from card numbers
or names. Token sources currently fail current admission; Rust also rejects nonempty
token links until their source contract is supported. The handoff grants no use
qualification and does not enable the prototype runtime loader for permanent owners.

The supported current ability mappings are evolve, feed, ride, evolve/feed and
evolve/ride entry variants, with exact root scopes and literal PP costs. Full payload
comparison rejects extra costs or conditions; identical numbers do not authorize
borrowing another operand. These variants require empty ports and no imports.
Real whole-card cases cover BP01-009, BP01-034 and BP01-092; the remaining variants
have synthetic mechanism tests, not real whole-card admission claims.
Source adapters accept only supported named constructs: offsets use registered
expression operands, never numeric or textual guesswork. Nonempty source or author
ports/imports are rejected by the source route until source-role/scope mappings are
specified. Concept, CardKind, Phase and negated-magnitude source conversions remain
unsupported; ambiguous direct `instead`, grouped Target quantity and grouped search
destination parameter sites fail closed. Pure local binding and import-graph checks do not establish those
source mappings.

The expanded profile shares the author graph through dynamic anchors and excludes
parameter and generated branches recursively. D0 consumes the expanded JSON tree;
the C0 sidecar is separate and is not a new public wire format. Candidate goldens
can exercise lowering, but production admission still requires complete use
dependencies and qualification checks. The owning face comes from SourceUse.owner;
card/face containers record placement, and a card-level ability on a multi-face card
does not imply an ability that remains active on every face.
Rust must verify the same profile's dynamic-scope positive
and negative cases with locked `jsonschema` 0.58.1; that Rust check is not yet complete.
BP08-038 and CP04-060 are pure expansion/canonical goldens only. Their existing
source fixtures remain negative cases (pending semantics and signed-number review,
respectively), not successful source admission.
