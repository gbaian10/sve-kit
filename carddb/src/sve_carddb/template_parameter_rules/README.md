# Template parameter recognition policy

This package implements [template-parameter-policy](../../../../docs/schema/template-parameter-policy.md).
It reads complete authored pairs from immutable Git blobs. It allocates no
template IDs, creates no approvals or translations, changes no SQLite schema
and is not connected to a build or preview.

`load_pairs(repository, authored_revision, main_revision=...)` validates closed
strict YAML, both canonical hashes, the entire directory and immutable ancestor
history, receipt event sets, precise triples and the original-eight bridge.
`load_config(config, repository, main_revision=...)` additionally requires the
explicit five-field `recognition_policy` pin, or an explicit null. Missing is
an error. Both revisions are full commit SHAs; every matcher must be reachable
from pinned main. Missing or shallow history fails offline. No worktree, latest
cache, network or live manifest supplies missing input. Loaded pairs retain
immutable bytes and exact dependency hashes; their model properties return
detached values. Loading proves structural consistency, not human reading,
source coverage or template adoption.

Only the eight registered `numeric-rule-proposals-v3` rules and seventeen
`parameter-rule-candidates-v1` rules are accepted. Conditions, roles and hashes
come from the existing matcher definitions. Policy cases exactly equal
`cases.fixed_examples(rule_id)`; tests and loader replay the same complete
synthetic fields. Partition, raw intervals, complete issues and old ownership
are recomputed; declared hints cannot override them. Notes are outside condition
hashes but inside the full policy/receipt hashes.

The original eight retain a complete v2 event with explicit null metadata.
Their current triples must be presented in a later actual authorization event,
with the single `reminder_fullwidth_sign_exclusion` restriction and evidence.
That later event authorizes its own IDs; showing the eight does not re-authorize
them. A finite AST adapter finds the original v2 source in pinned main, validates
every analysis function and matching grammar dependency, and replays its numeric
kernel without executing Git code. The sole permitted behavior change is the
fullwidth-sign guard in unchanged reminders. Source replay also requires zero
changed old numeric positions.

`replay(repository, recipe, stores, main_revision=..., legacy_bytes=...)` is the
separate complete-source API. `recipe` uses the existing `Recipe` envelope,
`id=template-parameters-jp-candidate-v1` and this package's `replay.CODE_PATH`.
Its config supplies the explicit policy pin/null, `source_recipes` from the
complete runtime pin generator, `source_batch={store_id,batch_id}`,
`legacy_file_hash` and `references` from the adopted glossary loader. The latter
includes `glossary.authored_revision`, exact index/all-shard hashes and
`exact_concepts_hash`. Config/program hashes and every runtime byte are checked.
Glossary files are reconstructed only from Git into a temporary tree; the
existing strict glossary loader and frozen translation evidence reader verify
their full closure. Optional `ProposalInputs(vocabulary,basis)` must match both
exact hashes already declared in `references`. Declared proposal inputs cannot
be omitted: they preserve the earlier candidate slot enumeration. Their
vocabulary roles remain proposed and pending; recognition does not adopt them.
Unadopted vocabulary is not guessed.

Every JP current source field is re-read, partitioned and checked with the
existing parser, source-span/UTF-8 verifier and candidate matcher. Both old
fingerprints and the complete use set must reproduce, with no extra members.
The first JP batch has its separate 3,669/13,913/14,782 baseline checks; other
explicitly scoped batches use their own legacy input. Each rule resolves only
its corresponding slot issue; other issues and reminder classification stay
pending. `Replay` contains hash/range/role proofs and remaining issues, never
source text. It produces no sampled adoption and preserves candidate-only
status. `compare_replays(previous, current)` compares full old numeric identities,
fingerprint/member bindings and every previously resolved slot's ownership,
coordinates, raw hash, role, value and concept ID/record hash in the same frozen
batch. A missing or rebound resolution raises `ResolvedSlotsChangedError`, with exact
text-free identities in `affected_slots`. Its `comparison` (also the successful
return value) separately lists `added_resolved_slots` and the current
`remaining_slots`; adding resolutions or changing pending causes is permitted.
Recipe/glossary upgrades must run this comparison before publishing results;
`replay()` alone cannot prove continuity against an earlier input pin.
Source coverage remains independent; presence v1 unknowns do not
become absent or empty fields.

Original message/page bytes and the actual authorized scope still require
verification at authored adoption, before publishing a real policy pair.
Structural CI cannot reconstruct a private conversation, assert that notices
were spoken or treat examples as human samples. This API does not fabricate or
substitute that evidence. The candidate CLI still emits pending proposals with
an explicit null policy; a candidate switch cannot activate formal recognition.
Classification and parameter recipe configs explicitly record null until a
separately authorized caller supplies a verified pair.
The top-level `config.recognition_policy` is the contract's five-field pin/null.
The candidate CLI's nested `candidate_classifier.recognition_policy: null`
describes pending proposals and never activates this loader.
