# Adopted glossary and digital names

This library implements the glossary boundary of translation-contract §2/§5.
It has no crawler, CLI command, application entry point or snapshot selection.
All source reads require explicit sealed batches, immutable Git recipe bytes and
F1 dependency/configuration pins. It never reads a research draft or latest cache.

`load_glossary()` checks the entire indexed entry and immutable decision histories.
Every non-indexed file is rejected, including temporary files. Candidates and bare
model reviews are rejected. A source value must match its declared language,
full-field exact hash and Unicode code-point span. Dictionary evidence must use
the same official key; effect evidence must locate actual effect fields. Official
names require an adopted same-card link to the exact digital and SVE faces.

`compile_build(("translation_evidence",))` creates the nine-table minimum closure
with T0 dependencies. It does not enable template, art or voice capabilities.
`import_glossary()` owns a transaction; `populate_glossary()` and `import_digital()`
compose in a caller-owned transaction. Authored index/shard bytes must match the
pinned authored commit. Historical choices and raw sources remain traceable after
withdrawal. Identical translated strings never merge different concept IDs.

`import_digital()` reads the explicitly pinned API inputs and only imports the
requested cards, required parent cards and all recognized name faces/languages.
Absent parents or inconsistent localized face inventories fail. Missing names stay
missing; no name, base-card ID or concept match is guessed. SV1 remains frozen;
there is no refresh implementation. Unrecognized additional face layouts fail
rather than being silently omitted. The SV1 card-page batch is not a fallback for
missing API names in this implementation.

`select_name()` prefers adopted SVWB names over SV1. Same-character/name-only,
unadopted, missing-language and unlocated face candidates cannot supply a name.
`populate_name_translation()` materializes a JP-to-Traditional-Chinese name only,
with exact frozen name evidence, deterministic IDs/52-bit revisions and the actual
review date. It preserves digital origin/authority and cannot supply an effect
translation. Ambiguous source names require an adopted context assignment. No
`current` or wording-ready filter discards published identity; callers compose with
`TextPlan.publication_identity()`, including pending wording and errata observations.

Context IDs describe source text and semantic variants, not owner eligibility.
A context hit does not make a digital translation usable for another card or face.
The #53 use/binding layer must revalidate the owner's adopted same-card, exact-face
link and frozen name evidence (through `populate_name_translation()`), then bind
only its returned translation ID to that owner. It must not select official names
by context alone or decode ownership from a render hash. Same-name owners without
an eligible link return no translation even when that context already has one.

Template rendering, regional source exceptions, binding/use, selection, public
export and English term adoption belong to later work. Vocabulary choices are
recognized by the strict wire/loader but glossary population rejects them until
the label projection is implemented; this rollback is atomic. The optional schema
capability does not claim complete importer/validator readiness. Real migration
counts, missing evidence and contract questions are reported separately in review
artifacts; synthetic tests are not production adoption receipts.
