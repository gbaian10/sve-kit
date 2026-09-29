# Permanent registry input

`storage.load()` retains the append tool's `(Index, entries)` API. Build consumers
use `snapshot.load_registry(authored_root)` to obtain the complete validated
registry before making any regional selection. The root must be a stable authored
checkout with an existing `ids/index.yaml`; unlike the allocator, a build reader
never bootstraps a missing index. Loading never allocates IDs or writes files.

```python
from pathlib import Path
from sve_carddb.registry.snapshot import load_registry

registry = load_registry(Path("authored"))
index = registry.files.index()
for record in registry.records.values():
    decision = registry.decisions[record.decision_id] if record.decision_id else None
    # Regional projection belongs after complete registry validation.
```

The reader loads only indexed shards. A directory inventory detects missing or
unindexed YAML and stops; it never adopts an unindexed file. It validates explicit
wire envelopes, canonical shard hashes, full decision membership and checked
sets, typed record data, global references, face mappings, per-region allocation
ranges and both allocation cursors. A malformed EN record prevents a JP consumer
from receiving a snapshot. No decision is recomputed for a regional subset.

`RegistrySnapshot.records` and `.decisions` are read-only mappings; record data
and nested evidence are frozen typed models with tuples. Each record retains its
inherited decision ID and indexed shard path. `files.shards` retains each shard's
canonical parsed JSON bytes and checksum, including the original decision
members, checked set and review precision. `files.index_content` retains the
complete canonical index. These are canonical parsed content, not YAML byte
hashes or official HTML hashes. The `index()`, `envelope()` and `entry()` helpers
return detached legacy models; editing a copy cannot alter the snapshot.

The current authored v1 kinds are card, face, printing, card_int_id,
region_mapping_review, art, card_related and source_correction. Their data remains
unchanged, including all JP and EN entries, provisional candidates, and active or
needs_review corrections. Source face maps are preserved as checked evidence;
face ordinals and card-number suffixes are not used to invent mappings. Permanent
IDs and integer allocations are never recalculated from current sorting.

Validation attests the consistency of historic registry evidence. It does not
read archived raw, open the live manifest, or verify current source freshness.
Cross-region identity decisions remain distinct from semantic equivalence and
fresh publication eligibility. A later importer must compare pinned observations
with verified source versions, report unavailable evidence and excluded records,
and satisfy the database's actual dependency closure. This reader does not apply
corrections or mark any build capability importer/validator ready.
