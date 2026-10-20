# Effect DSL

[effects.schema.json](effects.schema.json) is the sole grammar authority for
`astra/1`. Documents use `format: 1, kind: effect_set`; keyword registries
use `format: 1, kind: keyword_registry`. These envelopes select the existing
prototype grammar and keep Schema `$id: urn:sve-kit:effects:astra:1`.
The [author 1.0 contract](../docs/dsl/author-contract-1.0.md) specifies separate
`author_effect_set` and `effect_body_shard` entries. Their schemas and loaders
are not implemented yet. The prototype's 73 node variants contain 47 action atoms, 10 combinators,
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
`move.ref` also accept selectors; complete action-position schemas are not implemented here.
`first`/`second`/`rest` views still need their joint selector/target definitions.
The leader union shares `A.single-leader`, `A.multiple-leaders` and `A.leader-of`
without duplicate branches. The [selection shape matrix](../docs/dsl/author-selection-shapes-1.0.md)
defines Target role declarations and the named `RecipientSel` sum, and distinguishes quantity
rules for each position. The [quantity matrix](../docs/dsl/author-quantity-shapes-1.0.md)
defines closed single-group Target/choose/search quantity variants and counter costs.
Their source and binding/destination fields are present. The
[cost and target matrix](../docs/dsl/author-cost-target-shapes-1.0.md) adds ordered Cost
steps, payment wrappers, material variants, X declarations and grouped Targets.
Choose/search groups and other action extensions remain outside these fragments. Counter holder quantities are separate
from counter amounts, and only selection n/upto accept direct selection parameters.
DeckSlice accepts a player or player set and shares the same nonnegative-literal
Quantity as cost bounds and counter quantities. NumericExpr and Quantity reference
one shared NumericOperation union. CounterName has no parameter permission.
DeckSlice consumers are documented at action-level top; the complete move/banish/look/reveal
parents are not implemented; cost.banish.top uses DeckSlice. ObjectSel and Ref reject top/bottom aliases.
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
[effect matrix](../docs/dsl/author-effect-shapes-1.0.md) adds 26 closed EffectSpec keys,
cannot variants and rule overrides. `contracts/author_effects.py` shares the same
selector and numeric graph; grant remains rejected until complete recursive ability
headers and Steps exist. Effect parent defaults and source scope require C1/D0;
fragment acceptance is not enclosing-context validation. Cost/Target nodes do not constitute complete
ability headers or author entry contracts; full Sel and enclosing-context and
capability checks still require further contracts and C1/D0.
Snapshot count/exists do not establish hash equivalence with count_of/did. This recursive graph is not an expanded profile;
Rust decoding and execution have not been validated by this fragment.
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
The reader does not admit `author_effect_set` or `effect_body_shard`, expand parameters,
or provide the recursive `authored`/`expanded` profiles. This library is not
evidence of full author-language or runtime support.
