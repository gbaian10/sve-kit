# Wording adoption staging

This package parses the closed `wording-adoption-v1` receipt shape and its
immutable indexed inventory. It verifies record membership, confirmed decision
envelopes, historical chain references, and frozen evidence closure. These
checks establish receipt structure and provenance, not semantic equivalence or
an adopted current revision.

`classification` reports finite proposed rules with redacted hashes and Unicode
code point offsets. Every result remains `unapproved` with
`automatic_equivalence=false`, including a proposed `equivalent` action.
No rule approval is inferred from the category, identity review, or this code.

`scope.rebuild_raw_scope` reads every historical card source version in the
explicit sealed batches for every registered printing of a face and region.
It retains unknown effects and separately pins extractor and effect-presence
uses. Overlapping batches share an observation only when their exact metadata,
parser, extraction and presence results agree; their distinct batch uses remain.
`verify_raw_inventory` compares every receipt raw/projection pin against that
complete inventory. Corrections and corrected content hashes are a later check.

The caller must independently replay the full reviewed registry and context
before using a raw scope. Parsing a receipt or building a raw scope does not
validate the full review context, source corrections, ordering evidence or
mechanical predecessor. This package currently does not write authored files,
consume equivalence, create semantics, set `face_current`, or reuse DSL evidence.
Those require the remaining reconstruction, capability and transactional import
checks defined in authored-layout sections 9.2 through 9.7.
