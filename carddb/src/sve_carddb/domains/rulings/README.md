# Ruling source-use resolution

`resolution.build(documents)` preserves each original `applies_to` ordinal. T10
references without a proven old definition and complete domain stay reference-level
`unknown_legacy_scope`; IR references retain their own identity. Candidate IDs,
pattern hashes and historical inventory hints do not establish those proofs.

For a semantic rebuild, call:

```python
rebuild = Rebuild(
    legacy=(LegacyInputs(namespace, definitions, verify_old_definition),),
    current=current_frame_uses,
    verify_current=verify_current_source,
)
report = build(documents, rebuild)
```

Import `Definition`, `FrameUse`, `LegacyInputs`, `LegacyUse`, `Namespace` and
`Rebuild` from `sve_carddb.domains.rulings.mapping`, and `build` from
`sve_carddb.domains.rulings.resolution`.

The callbacks are the existing frozen-source build boundaries, not optional
approval checks. The old boundary proves the namespace's actual ID recipe and
normalizer, each real definition, and its entire explicitly applicable source
set. It must reject a missing or extra member, incorrect source, role or domain.
`Definition.uses=None` means its full domain is unknown; an explicitly empty
verified domain is an error. A subset cannot be presented as complete. Multiple
namespaces claiming the same unqualified reference stay reference-pending.

The current boundary verifies each `FrameUse` through the source resolver and
classifier used by its build, including exact owner, field, source version/hash,
canonical bytes, typed values, variant and occurrences. Reuse that build's
validated objects and checks; do not add another parser or normalization recipe.
A binding's structural validity alone does not prove source ownership. These
callbacks must raise for invalid declared data rather than convert it to pending.

For four-layer canonical/leaf changes, the caller keeps the old verified
frame/binding inputs before replacing the normalizer. Group old source uses by
actual frame ID into `Definition`s, carrying the old `semantic_variant`, exact
`LegacyUse.occurrence`, scope and parameter values. Pass the new classified
frame/binding pairs as `FrameUse`s. Scope roles and domains come from the existing
source grammar, not from observed card frequency. Pure NP/form rendering changes
do not require this rebuild.

The resolver maps only an identical source occurrence with an equal scope; the
old and new boundaries must therefore use the same role and domain codes.
It never chooses a target using a prefix, translated text, maximum coverage or
`supersedes`. Changed source fields, changed partitions, different scope,
unresolved semantics and multiple targets have explicit pending dispositions.
`Mapping` is many-to-many across definitions and source uses. Every complete old
use gets a resolution, including uses with no new candidate.

`report.payload()` separates reference counts from occurrence counts and includes
namespace descriptors, mappings and every old family under each new frame. It
records all parameter differences as hashes so historical string operands do not
leak official wording. New leaf schemas, old/new variants and pending flags remain
available for review. Reports are build outputs, not authored indexes or ledgers.

`report.applicable(occurrence)` selects only resolved edges for that exact source
use. An executable consumer calls `report.require(ruling_ref, occurrence)` and
must fail for pending or absent applicability; sharing a frame does not widen a
ruling. This does not grant DSL review, freshness or execution qualifications.

`storage.write(db, documents, revision, rebuild=rebuild)` uses the same resolver
and checks that target/candidate frames and their verified bindings exist in this
build before writing. `applies_to` may declare a `LegacyReference` containing the
complete old source-use domain; bare T references remain unproven. Mixing bare
and explicit references to the same T across rulings is rejected.

`historical.rebuild` validates every explicit domain against the frozen A1 catalog
and the adopted source fields on each ordinary build. Membership must be complete,
body hashes and lengths must match, and each printing must belong to the exact
source face. Only LF advances source line numbers. Cards are sorted lexically and
uses by canonical occurrence bytes. The translation build supplies already checked
and persisted current bindings; a build without that input rejects explicit domains.

Its callbacks retain these checked objects for the resolver; they do not perform
an independent second source validation. The original historical revision choice
still requires review against frozen full-face evidence, since identical body
bytes do not identify a historical revision. One-off evidence collection and
reconciliation scripts and outputs stay outside this repository.
