# Wording adoption

This package parses the closed `wording-adoption-v1` receipt shape and its
immutable indexed inventory. It verifies record membership, confirmed decision
envelopes, historical chain references, and frozen evidence closure. These
checks establish receipt structure and provenance. `replay_adoptions` then
independently reconstructs content, predecessors, equivalence and ordering.

`classification` reports finite diagnostic rules with redacted hashes and Unicode
code point offsets. Every result remains `unapproved` with
`automatic_equivalence=false`, including a proposed `equivalent` action.
No rule approval is inferred from a diagnostic category or identity review.
`policy.load_policy` separately verifies immutable paired YAML, canonical hashes,
the maintainer's approval and the supported fixed matcher contract. Only
`wp:eol-v1` can discharge complete CRLF/LF differences. Its matches must cover
the recomputed Unicode code point ranges of all candidates and the predecessor.
The other six rules leave human review pending. No source text is rewritten.

`scope.rebuild_raw_scope` reads every historical card source version in the
explicit sealed batches for every registered printing of a face and region.
It retains unknown effects and separately pins extractor and effect-presence
uses. Overlapping batches share an observation only when their exact metadata,
parser, extraction and presence results agree; their distinct batch uses remain.
`verify_raw_inventory` compares every receipt raw/projection pin against that
complete inventory. `Reconstructor` restores indexed registry and optional
product files from their own immutable revisions, verifies exact program/lock
dependencies and applies every confirmed active source correction after absence
projection. Applied and already-fixed results, image evidence and full observation
keys are independently reconstructed. Missing sources, unknown main text,
correction conflicts or unimplemented errata prevent confirmed replay.

The supported `wording-review-v1` configuration explicitly pins registry,
optional products, Python/Unicode versions, parser/projection, regions and a
complete errata cutoff day. Product identity and errata capabilities are explicitly
disabled. A historical program is accepted only when all its package Python and
lock/project bytes match the loaded runtime; unavailable implementations fail.
This does not execute an old checkout or substitute today's parser silently.

Ordering accepts complete first-availability days from every confirmed product
inclusion (an inclusion precision overrides its product), taking the earliest
day. Every adjacent edge and the independent previous-current edge must have
exact evidence or a named explicit adoption-order answer. Same-day, month/year,
unknown and incomplete catalog evidence cannot establish official order.
`source_update` is explicitly unsupported; it never falls back to fetched time.

`import_adoptions(db, **AdoptionInputs)` reloads and replays inputs within one
transaction. `populate_adoptions` composes within a caller-owned transaction.
Nonempty imports require the opt-in `semantics` capability and its T0 dependency.
The three semantic tables preserve the predecessor's complete rule text and
ordered sections with an independently checked immutable hash. Only checked
revisions share semantics. Unknown sections remain unknown and prevent DSL reuse;
token dependency normalization and DSL evidence migration are not implemented.
No import enables automatic engine support or invents rule effective dates.

Current selection retains the confirmed decision and explicit basis. Physical
observations remain tied to their own raw sources; correction revisions retain
their correction decision. New source keys never inherit checked status, even
with identical text. The current-build audit keeps the still-valid adopted current
and pending candidates separate; a known correction change invalidates it.

`prepare_adoptions` enumerates expected uses independently of DB writes.
`adoption_dependencies` and `adoption_configuration` preserve every historical
receipt, reviewed context and policy pin under portable qualified names. The
current review context and its qualified dependencies must also be supplied.
Compose returned uses with the identity/product/text inputs and validate the
complete union with `build_bundle.publish_bundle` / `verify_bundle`.
`adoption_report` records selections, predecessors and unchecked keys without
official wording. No entry point writes production authored receipts.
