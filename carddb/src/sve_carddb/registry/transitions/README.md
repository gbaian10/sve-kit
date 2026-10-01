# identity-transition-v1 loader

The wire contract is [identity repair](../../../../../docs/schema/identity-repair.md).
`load_transitions(authored_root)` is a read-only boundary for the complete
`identity-transitions/index.yaml` entrance. It does not apply repairs or authorize
publication. The caller supplies an immutable checkout or holds the existing global
registry lock for the entire read/plan/write operation.

The loader validates strict required fields (including explicit nulls), safe paths,
YAML restrictions, files smaller than 1 MiB, the exact indexed file closure,
canonical shard hashes, contiguous numeric sequence, one transition and one fully
confirmed decision per shard, complete membership hashes, previous refs, and exact
latest before refs produced by earlier transitions. It checks syntactic repair
cardinality, unique transaction participants, globally unreused repair IDs and
transition allocation anchors, the fixed UUIDv5 allocation recipe, route-state
shapes, and exact unreverted target refs and update-key sets for revert.

Each loaded shard retains exact YAML bytes and the complete canonical envelope.
`envelope()` returns a detached copy. Index bytes are retained separately; no
registry, authored file, archive, or source cache is changed. A missing directory
is supported for legacy checkouts; an existing entrance must have its index.
An explicit empty index has `includes: {}`. Unindexed files, including interrupted
writes (such as `003.yaml.tmp-abc`), hidden files, and unknown extensions, fail closed.

This stage does **not** verify original registry before refs or append-only Git
bases, F1 program/dependency/configuration reconstruction, archived evidence,
effective face/art ownership and move coverage, effective repair graphs, source
freshness, or semantic revert dependencies/inverse states. Loading a confirmed
receipt means its envelope is valid; it does not mean these claims have been
verified. The example program in the contract uses illustrative F1 pins; real
inputs follow `BuildContext` (nonempty dependency pins and canonical JSON text
configuration), as required by source-archive §2.2.1.

Stage B must address decision/reference hashes currently computed from model
serialization, checking the original canonical envelopes against any normalization.

Until effective projection is implemented, existing registry readers and append
planners reject every nonempty transition entrance rather than building from the
old unmodified registry. Empty entrances keep existing behavior. No public
identity-change row, route alias, snapshot schema change, or registry writer is
introduced here. Replay, fixed-order persistence, publication history, and full
merge/split/reassign/revert acceptance remain subsequent work under #37.
