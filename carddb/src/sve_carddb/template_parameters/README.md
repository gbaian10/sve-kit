# Current template parameter candidates

`inventory.build()` classifies `parameter_schema` and `source_span` for the installed
parser's scan of explicit sealed JP card batches. It receives reference data and
an explicit enabled-rule selection, returning candidates, field/span proofs,
matched rules and remaining causes. It does not allocate IDs, adopt definitions,
write translations or infer missing concepts. The public field contract remains
[translation-contract §4](../../../../docs/schema/translation-contract.md).

The historical CLI, catalog comparison, producer/environment pins and evidence
writers have been removed. No old catalog file or historical replay is required.
Body, reminder and token header retain their established normalized spellings.
Layout uses one exact-whitespace literal parameter. Source spans use code-point
half-open intervals; verification restores every raw value, UTF-8 byte range and
literal fragment without silently guessing a role or dropping a position.

`analysis`, `numeric_rules`, `rule_candidates` and `candidate_matching` contain the
finite current grammars. Signed, identifier, ordinal and reminder exclusions run
before ordinary numeric ownership; new matchers cannot claim an already-owned
position. Unit suffixes reject ordinal forms, ASCII continuations and recovery
spelled with the times unit. Numeric values use JavaScript's safe unsigned range;
ordinal roles start at one, other magnitudes at zero. Signs remain literal text.
Header roles follow the full named grammar rather than nearby-character guesses.
Conditions and closed vocabularies are tested against synthetic positive/negative
cases. They belong to the installed program, not an approval receipt or a frozen
producer revision.

The enabled switches in `template_parameter_rules.current` control recognition.
An empty selection enables no rules. Enabled rules directly classify the exact
source position; unmatched reasons remain on the slot. Disabled ordinary numeric
rules report `numeric_rule_disabled`. There is no serialized resolution step,
approval status or per-slot hash wrapper. Candidate completeness is separate from
full source coverage and never implies an adopted definition or an active
translation.

`current_references.adopted()` reads the shared current glossary and validates
source-backed exact names before lookup. Card-name and term references require a
unique exact concept; no trimming, NFKC, case folding or card-ID guessing occurs.
The current build adapter resolves a recognized card-name reference without a
concept to its own exact spelling, which verification checks against the source;
an ambiguous concept remains pending.
The current vocabulary adapter requires active derived catalog codes. Composite
vocabulary keeps its separate roles, and unknown or ambiguous references remain
pending. Substring mentions are diagnostics, not semantic bindings.

`verification` checks complete positional coverage, raw values and source span
bounds, slot types and reference kinds. Repeated slots must have equal
values; malformed or missing occurrences cannot become successful parameters.
Current builders reuse this same classifier and verifier.
