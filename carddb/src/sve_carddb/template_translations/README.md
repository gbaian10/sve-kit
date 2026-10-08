# Current template definitions and translations

`current.read_templates(repository, revision)` reads the fixed working-tree directories once.
It checks safe regular files, strict YAML, canonical include hashes, complete file
closure, closed record shapes, unique selection keys and structural references.
It does not read raw sources, traverse ancestors, or require adoption receipts,
initial sampling, model-review agreement or historical producer environments.
Text, notes and quality flags can be edited in a new commit. Low-confidence
machine translations remain active values; consumers must mark them for review
and let readers switch to the original source.

Current template shards use `translation_authored_format: 2`. Each record has
`kind,data` plus optional `origin,low_confidence,note`. Omitted quality values
are `project` and `false`; YAML writers omit them. Selection keys are derived
from identity fields, and stored `record_key` fields are rejected. Definitions hold
`id,normalized_hash,role,source_lang,normalizer_version,semantic_variant,parameter_schema,content_hash`;
translations contain only `template_id,lang,text`.
`origin` remains `machine` after review. Notes do not affect semantic identity.
The shared glossary reader owns glossary/name values; template code uses that
snapshot instead of maintaining a parallel glossary.

Unresolved original drafts use `template_translation_candidate` in their dedicated
indexed shard area. Their source kind, stable draft ID, language, exact target text,
`normalized_hash`, `role` and sorted reason codes are closed data. Anonymous N/X and
malformed placeholder syntax remain untouched; candidates need no definition ID.
They never become active targets, variants, bindings, pins or translated coverage,
even with low confidence false. A full build fails when a candidate's pattern has
no current source position.

`current.validate_templates(inputs, sources, batches)` is a separate full build
operation. `current_sources.Sources` uses the installed parser, normalizer, reference
data and explicit current rule switches to enumerate the build's own sealed JP batches
once. Source positions are generated at build time and never stored in Git; their
entry IDs exclude the store and archive batch, so resealing the same pages keeps them.
Full source coverage is reported separately: presence-v1 unknown pages still make
coverage incomplete.

`normalized_hash` is the SHA-256 of the normalized pattern; with `role` it selects
the source positions a definition can match. Authored files keep only this hash,
never the official normalized text. A definition matches every position in that
group whose schema verifies; a position with another schema stays unmatched rather
than merged. The build fails when no position verifies, or when verified positions
disagree on slot semantic roles. Definitions must reproduce the six-field semantic
payload and its content hash, slot types, semantic roles, safe integer bounds, raw
values and full positional coverage. Repeated slots require equal values and roles.
Unknown references stay pending. A definition ID is `T` followed by a prefix of its
content hash; one payload has one allocated ID and collisions compare full payload
bytes. A source position can match at most one current definition.

`template_parameter_rules.current` reads registered, explicit enabled/disabled
switches from `authored/template-parameter-rules/current.yaml`. Missing files fail;
an empty rule list does not enable all matchers. Conditions and recognition roles
belong to the current program, not an approval/hash chain. Disabled rules leave
positions pending. This does not relax unrelated identity, correction or registry
adoption rules.

`text.parse()` retains the finite placeholder language. Parameters are exactly
`{{slot_name}}`; braces and backslashes in literal text must be escaped. Unknown,
unused or malformed slots fail. No expressions or global N/X substitutions exist.

Historical adoption loaders, stored inventories, approval receipts and frozen
semantic interpreters have been removed. Git retains their history; current builds
need no legacy template catalog or historical environment. Shared member, schema
and matching checks live in `members` and `definitions`.

`current_build.apply()` is the export-offline consumer. It projects the verified
definitions and current targets into the shared current DB schema, then renders
every Japanese main text and section of face revisions and printed faces whose
exact source hash a template source covers. Eligibility is the source hash and a
confirmed card identity, not `printed_text_state`. One source text has one
context and selection; a differing reading of an already translated text stays
original. `current_render` requires the whole field, appends anchored reminders
once, preserves layout, and returns fallback issues instead of a partial
translation when any fragment, placeholder or reference label is missing. A
translation with invalid placeholders falls back per field rather than failing
the build. Reference labels come from glossary choices, selected vocabulary-label
translations and selected name translations. A recognized card-name reference
without a glossary concept keeps its own source spelling or that text's selected
name translation and marks the field low confidence; an ambiguous concept stays
pending. Low confidence propagates from definitions, translations, labels, low
confidence recognition rules and these fallbacks. Named variants are selected only
by explicit pins. `render-v2` identity excludes notes, receipts and the
representative source locator. Unofficial card text does not gain an official
authority from an individual official reference label. Frozen raw inputs for CI must come from the project's
private testdata input, never a maintainer-only path or a live website.
