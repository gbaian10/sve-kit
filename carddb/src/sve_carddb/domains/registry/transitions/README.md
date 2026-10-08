# identity-transition-v1 loader

The wire contract is [identity repair](../../../../../../docs/schema/identity-repair.md).
`load_transitions(authored_root)` is a read-only boundary for the complete
`identity-transitions/` directory. It does not apply repairs or authorize
publication. The caller supplies an immutable checkout or holds the existing global
registry lock for the entire read/plan/write operation.

The loader validates strict required fields (including explicit nulls), safe paths,
YAML restrictions, files smaller than 1 MiB, contiguous numeric sequence, one
transition per shard, previous refs, and exact latest before refs produced by
earlier transitions. It checks syntactic repair
cardinality, unique transaction participants, globally unreused repair IDs and
transition allocation anchors, the fixed UUIDv5 allocation recipe, route-state
shapes, and exact unreverted target refs and update-key sets for revert.

Each loaded shard retains exact YAML bytes and the complete canonical envelope.
`envelope()` returns a detached copy; no registry, authored file, archive, or
source cache is changed. A missing or empty directory has no transitions. Any
other file, including interrupted writes (such as `003.yaml.tmp-abc`), hidden
files, and unknown extensions, fails closed.

This stage does **not** verify original registry before refs or append-only Git
bases, F1 program/dependency/configuration reconstruction, archived evidence,
effective face/art ownership and move coverage, effective repair graphs, source
freshness, or semantic revert dependencies/inverse states. Loading a transition
means its envelope is valid; it does not mean these claims have been verified. The example program in the contract uses illustrative F1 pins; real
inputs follow `BuildContext` (program revision and canonical JSON text
configuration), as required by source-archive §2.2.1.

Until effective projection is implemented, existing registry readers and append
planners reject every nonempty transition directory rather than building from the
old unmodified registry. An empty or missing directory keeps existing behavior. No public
identity-change row, route alias, snapshot schema change, or registry writer is
introduced here. Replay, fixed-order persistence, publication history, and full
merge/split/reassign/revert acceptance remain subsequent work under #37.

## Effective apply replay (stage B)

`replay.replay(authored_root, inputs)` provides a separate read-only effective
apply boundary. It returns `EffectiveRegistry`, not a `RegistrySnapshot` that
legacy build/import paths could accidentally consume. The caller still supplies
an immutable checkout or holds the global registry lock for the whole read.
`ReplayInputs.registry(basis)` reads the exact immutable historical Git revision;
`ReplayInputs.routes(transition)` independently extracts frozen source facts for
that transition's reviewed context. These adapters must not copy the proposed
`routes` or substitute the current checkout for a historical registry. The core
checks the basis index hash, global IDs, allocation cursors, byte-preserving
append-only bases and exact effective before references.
Both canonical and exact original shard bytes remain pinned.

Each transaction is applied to detached memory and checked as a whole. Stable
owner/ID/region/card number/variant/layout/face/art ownership fields cannot change.
`card_int_id` is never an update target. Only explicit repair destinations may
allocate new card/face/art IDs, with the loader's fixed allocation recipe and
collision checks against the full current original registry and all earlier
transition allocations. Replaying identical inputs returns identical state and
writes nothing.

Merge, multi-target split and reassign must exactly explain all parent changes
and retirements. Complete source face maps (including double-faced backs), all
old faces and all old art must be listed. Art targets and remaining uses partition
the actual previous uses; destination art keeps its existing uses. Old faces and
art retain their parents. The effective apply graph rejects cycles, and retired
cards cannot be restored by apply. Source corrections still tied to an old face
block an affected result; no correction, wording, baseline, DSL or translation
approval is inherited by changing IDs. A known old art cannot be discarded;
an unknown art cannot be made known by the move itself. Independent fresh art
adoption, including any such promotion, remains a later adapter responsibility.

`EffectiveRegistry.entries()` retains build-side tombstones, faces and unused art.
`browse(kind)` filters retired cards/their faces and art
without an included current use. It does not expose artists, baselines, current
revisions or automated capabilities. Wording/errata state is independent of this
identity view: consumers of the text pipeline still use `publication_identity()`
to preserve known identity when wording is pending. `resolve_int_id(value)` keeps
the original printing and supplies repair hints; a split returns `choice_required`
and the destination choices instead of silently selecting or replacing a printing.
After a split, a later explicit reassign conservatively keeps `choice_required`;
retired destinations are removed from the remaining choices.
It is a consumer hint, not a deck rewrite operation.

Route facts explicitly distinguish official, provisional and unknown numbers;
unknown cannot prove an official number. Provisional entries use the unchanged
permanent integer ID. Official keys come from independent exact source facts,
not from an authored override of `printing.card_no`. Every transition's route
facts must name source versions pinned in its evidence. The complete expected
before/after states must match `routes`. Old keys are retained forever and resolve
directly to the same printing's latest canonical, including after two renumbers
or provisional-to-official correction. Alias chains, returning canonical keys
that collide with their own aliases, hijacking another printing's old key and
exact collisions fail closed. Exact-number variant overrides require a separate
adopted-source adapter; this core conservatively refuses ambiguous variants.
Card merging by itself does not create a route alias.

### Remaining gates

All old `load`/`read_registry_files`/`load_registry`, append planner (including its
preloaded input) and relayout guards remain unchanged for nonempty transitions.
`storage.read_base_files` reads only original envelopes for the replay boundary;
it is not a replacement builder/reader and does not apply transitions. An empty
or missing directory retains the existing complete build/publication path and
also works with the new replay core.

This stage does not implement a production historical Git/F1/archive adapter,
source freshness, target coverage reconstruction, complete capability dependency
reconstruction, DB/snapshot event export, first-publication evidence or the
fixed-order write/recovery transaction. Full public artist/baseline
reference closure belongs to the future DB/snapshot adapter; this view cannot
make those rows public. Real registry repair/publication remains blocked until
those gates are implemented and reviewed. Named revert fails closed in this core;
inverse state, dependency closure and route restoration are stage C.

The loader checks parsed raw canonical bytes against model serialization before
using model-based reference hashes, rejecting any normalization. `0001.yaml`
remains accepted: the approved wire contract says *at least* three digits, not a
unique minimal padding. Other files of any extension (including temporary and
hidden files) remain rejected.

Synthetic acceptance and counterexamples:

```bash
uv --directory carddb run pytest tests/test_identity_replay.py tests/test_identity_replay_history.py tests/test_identity_replay_rejections.py tests/test_identity_transition_loader.py --durations=20
```
