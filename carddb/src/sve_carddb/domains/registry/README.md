# Permanent registry input

`storage.load()` retains the append tool's `(Index, entries)` API. Build consumers
use `snapshot.load_registry(authored_root)` to obtain the complete validated
registry before making any regional selection. The root must be a stable authored
checkout with an existing `ids/index.yaml`; unlike the allocator, a build reader
never bootstraps a missing index. Loading never allocates IDs or writes files.

```python
from pathlib import Path
from sve_carddb.domains.registry.snapshot import load_registry

registry = load_registry(Path("authored"))
index = registry.files.index()
for record in registry.records.values():
    ...  # Regional projection belongs after complete registry validation.
```

The reader loads every YAML shard under `registry/` and `ids/`; `ids/index.yaml`
holds only the allocation policy and both cursors. It validates explicit wire
envelopes, typed record data, global references, face mappings, per-region
allocation ranges and both allocation cursors. A malformed EN record prevents a
JP consumer from receiving a snapshot.

`RegistrySnapshot.records` is a read-only mapping; record data and nested
evidence are frozen typed models with tuples. Each record retains its shard path.
`files.shards` retains each shard's canonical parsed JSON bytes and content hash,
and `files.index_content` the canonical index. These are canonical parsed
content, not YAML byte hashes or official HTML hashes. The `index()`,
`envelope()` and `entry()` helpers return detached models; editing a copy cannot
alter the snapshot.

Registry records have no separate review envelope: every record is a manually
confirmed identity, and a source correction's own `state` says whether it is
`active` or still `needs_review`.

The current authored v1 kinds are card, face, printing, card_int_id,
region_mapping_review, art, card_related and source_correction. Their data remains
unchanged, including all JP and EN entries, provisional candidates, and active or
needs_review corrections. Source face maps are preserved as checked evidence;
face ordinals and card-number suffixes are not used to invent mappings. Permanent
IDs and integer allocations are never recalculated from current sorting.

Validation attests the consistency of historic registry evidence. It does not
read archived raw, open the live manifest, or verify current source freshness.
Cross-region identity mappings remain distinct from semantic equivalence and
fresh publication eligibility. A later importer must compare pinned observations
with verified source versions, report unavailable evidence and excluded records,
and satisfy the database's actual dependency closure. This reader does not apply
corrections or mark any build capability importer/validator ready.
