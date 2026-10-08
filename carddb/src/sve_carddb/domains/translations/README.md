# Glossary and digital names

The translation entry accepts format 2 only. Shards contain
`kind/data` with optional `origin/low_confidence/note`. Omitted quality values
are `project` and `false`. Selection keys are derived on load; stored `record_key`
fields are rejected. YAML writers omit these defaults and a null
`missing_source_reason`, while retaining meaningful `value: null`.
Readers load the current working tree once from the dedicated glossary/templates/
overrides directories. They validate size, types, unique keys and references,
then sort; shard gaps and unordered records are accepted. There is no checksum
index or Git-byte gate. Selection keys and concept IDs stay stable when wording
or notes change.

Use `build.t1.compile_build()` for current imports.
`import_glossary()` owns an atomic transaction; `populate_glossary()` composes
inside a caller's transaction. Exact frozen sources, language, code-point spans,
owner applicability and official same-concept evidence are checked before
projection. Imported rows retain authored-source provenance without creating
review decisions. Research drafts, latest cache and network are never inputs.
Dictionary evidence must use the same key; effect evidence must locate real
skills or effects. An official digital name requires an adopted same-card link
to the exact digital and physical faces. Origin and low confidence are independent;
valid machine wording remains machine after review and low-confidence wording
is usable. A project source claim never grants official authority.

`names.sources` keeps owner-local source lookup for names, links and templates.
It verifies confirmed identity, exact language and content hash. Known printed
names can differ from current names; unknown or omitted printed text cannot borrow
another source. `names.identity.IdentityEvidence` binds current concept associations
to the already loaded current registry, the physical source-face map and its complete frozen
observation. Nonempty identity transitions still require a complete supported
adapter. `names.resolve.prepare()` checks current assignments against this build;
a changed source cannot inherit a stale assignment. Homonyms need explicit semantic
variants; non-Japanese assignments require their own current concept association.

`names.candidates.prepare()` produces current terms and choices from explicitly
selected keys. It checks the complete existing glossary, exact frozen JP name
references and source claims. It allocates no card identity or same-card relation.
`glossary_emphasis_choice` holds the current Bool or None for a rule term; other
categories derive emphasis from their type, never their key prefix. None means
missing emphasis, not false or a reason to discard the available wording.

`import_digital()` imports only explicitly requested frozen cards, required
parents and recognized name faces/languages. Missing names remain missing;
unrecognized or inconsistent face inventories fail. `name_proof()` verifies the
actual frozen face and language. `names.counterparts.first_counterpart()` keeps sv1
before svwb; ambiguous adopted names fail. Human digital links keep their own
name evidence and are checked against the current catalogue. Same-character or
same-name browsing cannot supply translation authority; a shared context never
grants another owner eligibility.

Current name policy application is in `domains.digital.name_policies`. Public projection
is a separate coordinated contract/producer/reader boundary; this library does not
invent review state or adapt current quality into an obsolete public shape.

Shared authored inputs and closure checks live in `inputs`; exact evidence checks
live in `glossary.evidence`, and glossary DB projection lives in `glossary.populate`.
Template loading, building, rendering and writing live in `templates`. Parameter
recognition and matcher switches live in `parameters`; the generated source
inventory lives in `source_inventory`. Shared source, owner and quality types live
in `models`. These Python groups do not change authored paths or formats.
