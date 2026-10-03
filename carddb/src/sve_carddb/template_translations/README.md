# Template translation preparation

`text.parse()` implements the finite translation language from
`docs/schema/translation-contract.md` §4.2. A parameter is exactly
`{{slot_name}}`; literal braces and backslashes must be escaped. It rejects
unknown or unused slots, malformed braces, invalid escapes and expressions.
Source and target positions use Unicode code points, including astral characters.

`preparation.convert()` rewrites a legacy draft using an explicit target alignment.
The alignment pins both the exact draft text hash and the canonical parameter
schema hash. Sorted, disjoint target spans must match their raw hashes and cover
each declared source occurrence exactly once. This conservative migration helper
requires occurrence preservation; the general text parser only requires each slot
to be used. Target slots may reorder the source slots. Literal `N`/`X` characters
outside the supplied spans remain literal. No ordinal pairing or global letter
replacement establishes a semantic correspondence.

The resulting `Prepared` value always remains `candidate_only` with machine origin.
It keeps the draft's confidence for private preparation and never contains a
decision, membership, policy adoption or model review. These candidate fields
are not the authored `template_translation.data` wire format. Do not put candidates
or legacy drafts into `authored/translations` or a publication database.

`review.verify()` checks a separately recorded review against the final exact text
hash. Changing placeholders, escaping or wording requires another review. The
closed model records both models and versions and distinguishes agreement from an
unresolved dispute; a legacy bare `ok` verdict is not this review. The dispute
guard requires both a recorded human resolution and an actual sample declaration.
It does not validate the declaration's provenance or create a human event.

These functions are prerequisites for template intake. Frozen-source reconstruction,
definition/schema adoption, immutable authored closures, actual human sample events,
translation-policy loading, database import and rendering still require their
respective validators. Neither model agreement nor a parameter-recognition approval
grants those decisions. Keep original/final translation bytes and alignment review
evidence private until a valid authored batch can be produced.
