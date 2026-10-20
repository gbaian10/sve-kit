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

[author-nodes.schema.json](author-nodes.schema.json) currently defines only the
closed author leaf parameters (`A.param`, declarations and eight typed value
variants). Its root rejects standalone documents; select a named `$defs` entry.
[effects-index.schema.json](effects-index.schema.json) defines the complete
`format: 1, kind: effects_index` envelope and record syntax. Both use JSON Schema
2020-12. The `AuthorSchemas` reader in
`carddb/src/sve_carddb/domains/effects/author_inputs.py` takes the explicit schema
directory and validates these inputs with local resources only. Python leaf
types are checked against every implemented definition by structural drift tests.

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
It does not admit `author_effect_set` or `effect_body_shard`, expand parameters,
or provide the recursive `authored`/`expanded` profiles. This library is not
evidence of full author-language or runtime support.
