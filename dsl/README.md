# Effect DSL

[effects.schema.json](effects.schema.json) is the sole grammar authority for
`astra/1`. Its 73 node variants contain 47 action atoms, 10 combinators,
14 declarations and two loading/unsupported markers.

The Rust prototype validates `authored/keywords.yaml` and every pack file, expands
macros, then validates the expanded programs again. `carddb` does not yet invoke
this schema and Rust types are not generated from it.

See [DESIGN.md](../DESIGN.md) for semantics and [KNOWN_LIMITS.md](../KNOWN_LIMITS.md)
for the executable subset. Schema validation does not establish card-text
correctness or runtime support.
