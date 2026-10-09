# Effect DSL

[effects.schema.json](effects.schema.json) is the sole grammar authority for
`astra/1`. Documents use `format: 1, kind: effect_set`; keyword registries
use `format: 1, kind: keyword_registry`. These envelopes select the existing
prototype grammar and keep Schema `$id: urn:sve-kit:effects:astra:1`.
A future DSL 1.0 uses a separate kind or entry, rather than effect_set format 2.
Its 73 node variants contain 47 action atoms, 10 combinators,
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
