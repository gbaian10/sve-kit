# Effect DSL

The [author 1.0 contract](../docs/dsl/author-contract-1.0.md) specifies separate
`author_effect_set` and `effect_body_shard` entries. Persisted handwritten inputs
use format 2 through [author-storage.schema.json](author-storage.schema.json) and
[effect-body-storage.schema.json](effect-body-storage.schema.json).
[program.schema.json](program.schema.json) defines the expanded public DSL 1.0
programs carried by snapshots 4.1.0 and 4.2.0. Changes to author ability variants must also
update this schema; its closed public shapes exclude author and source evidence.
Direct admission uses `admission: light | shared` on a card, use or named body.
Equivalent wording may share a body without a frame adapter or machine anchors.
Whole-card line coverage, permanent identities, schema and runtime support remain
required. See [authoring tools](../docs/dsl/authoring-tools.md).

The [keyword registry](../docs/schema/domains/keyword-mechanics.md) supplies card
mechanic concepts and the engine's rule names. The Rust loader lowers published
expanded AST to checked IR. Execution admission is checked separately.

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
separates required and optional payload fields from history.where for each
event key. The
[effect matrix](../docs/dsl/author-effect-shapes-1.0.md) adds 27 closed EffectSpec keys,
cannot variants and rule overrides. The [ability matrix](../docs/dsl/author-ability-shapes-1.0.md)
defines complete ability heads, the granted-head subset, conditional play modifiers,
and delayed triggers. Its recursive definitions share the ability, effect and
step graph. Grant payloads can combine keywords and abilities and
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
CardDB and the Rust lowerer use the same expanded schema profile. That profile
rejects residual parameter and generated ability nodes. The reader keeps the
validated JSON tree and its authored values. CardDB checks review metadata at
printed keyword entries. Rust decodes the tree into the author's typed AST
and checks bindings, lowering and runtime capabilities at their own boundaries.

The reader rejects non-JSON YAML values, unsafe numbers, duplicate/non-string
keys, unknown fields, wrong envelope versions and duplicate index card/scope
pairs. Diagnostics expose structural paths and available YAML coordinates without
source excerpts. Parameter placement, source-role compatibility, candidate-file
closure and registry membership are separate checks, not implemented by this reader.
Concept values are nonempty author strings, including Japanese trait/class names;
they are not restricted to the ASCII `Code` syntax used for source-slot names.
For non-string keys and merged keys without direct cursors, coordinates identify
the containing mapping; ordinary property paths locate their value node.
Reference syntax does not validate binding scope, event-field availability,
singleton cardinality or permission to follow a movement receipt.

## Direct author handoff

The storage schemas cover format 2 main shards and named bodies. `load_author_set`
checks the two-way index closure and unique body editions. The current build
supplies permanent JP identities, current revisions and the complete rule-line
inventory.

Named bodies substitute declared parameters before expanded-schema validation.
The expanded profile shares the author graph through dynamic anchors and
excludes parameter and generated branches recursively. Each expanded ability
carries a direct position in its permanent face. Independent keyword declarations
retain their original order and direct source coordinates.

`CardAccepted.lowering_input()` hands Rust the expanded tree, permanent identity
and complete face and line inventory. `ExpandedLowerer.lower_current` checks the
coverage and lowers the supported ability shapes. Game admission checks the
whole card before using it in a match. The author and runtime interfaces are
specified in the [author contract](../docs/dsl/author-contract-1.0.md).
