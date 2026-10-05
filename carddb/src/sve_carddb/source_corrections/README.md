# Frozen source corrections

Pass `images=FrozenImages(store_root, store_id, batch_id)` to
`plan_text_observations(identity, texts, images=images)`. The image provider resolves
the preserved regional `img src` and exact authored image hash to a sealed source
version, including historical versions. Missing or changed evidence fails; the
provider never visits live/latest or the network. A scoped correction cannot be
staged without explicit image evidence. The complete authored envelopes remain
unchanged, including corrections outside the selected output region.

The importer reuses `registry.corrections.correction_status`. An active correction
first checks for an upstream fix, then requires both exact old value and the
complete `registry-observation-v1` hash. Proposed corrections are retained without
an application. Conflict applications have no successful result and the build
record becomes `needs_review`; upstream fixes become `upstream_fixed`, keep the
existing result and report a retirement warning.

`populate_text_observations` writes `source_correction`, `correction_evidence` and
`correction_application` in the same transaction as the text graph. Successful
effect corrections link their result text and revision; type corrections link
their rule revision. Raw observations keep their original revisions. Corrected
revisions have separate IDs, `change_kind=source_correction`, the correction
decision and an original-revision `supersedes_id`; no effective date is invented.
Replacement occurs before candidate comparison, without filling printed text or
claiming wording equivalence. All exact raw/current-bearing fields remain in the
immutable input plan; missing effects still obey the existing report-only rule.

F1 includes each correction comparison and image evidence use, with its exact
locator, parser, batch, descriptor and first receipt. The pinned configuration
includes the complete correction record hashes and image pins. Correction content,
reason, membership or state changes require rebuilding and invalidate the old
candidate/configuration; this API does not consume or reuse wording `checked`
receipts. `verify_corrections` checks table inventories, exact values, observation
links and result content beyond the SQLite adoption/FK checks.

`correction_references(db, plan, vocabulary)` returns the affected
printing/face/source/revision uses and public `Correction` values for the snapshot
projector. Its outer source ID is an internal join key, not a public snapshot FK.
Only `applied` references get a marker. Shared text units and unaffected uses get
none. `source_url` is always present and comes from the official page; an unavailable
URL stays null and never becomes an image URL.

`plan.publication_identity()` closes correction conflicts over regional
identity references. An active correction whose parent printing was already
excluded by identity validation also blocks its regional card, so sibling
printings cannot bypass the unresolved correction. `correction_exclusions(db, schema, plan)` also closes these
seeds over every enabled FK, including routes, aliases and default overrides.
These methods do not use `plan.diagnostic_exclusions`: pending wording remains visible under
authored-layout §9.1. The DB retains blocked rows as diagnostic/source history;
the eventual snapshot projector uses the blocked keys to preserve a closed output.
This package supplies correction staging and reference values, not the full
snapshot serializer or release validation.
