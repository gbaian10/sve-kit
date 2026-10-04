# Current template definitions and translations

`current.read_templates(repository, revision)` reads the indexed current Git tree.
It checks safe regular files, strict YAML, canonical include hashes, complete file
closure, closed record shapes, unique selection keys and structural references.
It does not read raw sources, traverse ancestors, or require adoption receipts,
initial sampling, model-review agreement or historical producer environments.
Text, notes and quality flags can be edited in a new commit. Low-confidence
machine translations remain active values; consumers must mark them for review
and let readers switch to the original source.

Current template shards use `translation_authored_format: 2`. Each record has
`record_key,kind,data,origin,low_confidence,note`. Definitions preserve their nine
semantic/source fields; translations contain only `template_id,lang,text`.
`origin` remains `machine` after review. Notes do not affect semantic identity.
The shared glossary reader owns glossary/name values; template code uses that
snapshot instead of maintaining a parallel glossary.

Unresolved original drafts use `template_translation_candidate` in their dedicated
indexed shard area. Their source kind, stable draft ID, language, exact target text,
current inventory IDs and sorted reason codes are closed data. Anonymous N/X and
malformed placeholder syntax remain untouched; candidates need no definition ID.
They never become active targets, variants, bindings, pins or translated coverage,
even with low confidence false. Missing source counts remain unknown. Referenced
inventory entries must exist and full builds still verify their entire raw closure.

`current.validate_templates(inputs, sources)` is a separate full build operation.
`current_sources.Sources` uses the installed parser, normalizer, reference data and
explicit current rule switches to enumerate every declared frozen JP batch once.
It reuses parsed documents within that build. Inventory format 3 contains only
`template_source_format,kind,source_batches,entries`; no producer or output-manifest
pin is needed. Every actual entry must appear exactly once and equal its declared
source position. Full source coverage is reported separately from positional agreement:
presence-v1 unknown pages still make coverage incomplete even when all known
source positions agree. Parsing an inventory is not a successful build.

Definitions must reproduce the six-field semantic payload and its content hash,
exact source spans, slot types, semantic roles, safe integer bounds, raw values
and full positional coverage. Repeated slots require equal values and roles.
Unknown references stay pending. One payload has one allocated ID, collisions
compare full payload bytes, and a source position can match at most one current
definition. Existing IDs stay allocated even when unused old collision definitions
are removed. Supersedes links cannot cycle; an unloaded legacy parent does not
create a fictitious foreign-key row.

`template_parameter_rules.current` reads registered, explicit enabled/disabled
switches from `authored/template-parameter-rules/current.yaml`. Missing files fail;
an empty rule list does not enable all matchers. Conditions and recognition roles
belong to the current program, not an approval/hash chain. Disabled rules leave
positions pending. This does not relax unrelated identity, correction or registry
adoption rules.

Flavor uses the independent `flavor-exact-v1` normalizer: one whole field, code-point
span, zero slots, no trimming, NFKC or N/X replacement. Unknown, exact empty and
fixed-White_Space-only fields remain distinct. `current_owners.Owners` verifies each
paragraph against its own confirmed printing and physical face observation, never
another card's equal paragraph or the current effect. Missing owners stay pending.
Final flavor display uses LF and rejects outer or trailing line whitespace;
invalid bytes are not silently normalized.

`text.parse()` retains the finite placeholder language. Parameters are exactly
`{{slot_name}}`; braces and backslashes in literal text must be escaped. Unknown,
unused or malformed slots fail. No expressions or global N/X substitutions exist.

Historical adoption loaders, inventory formats 1/2, approval receipts and frozen
semantic interpreters have been removed. Git retains their history; current builds
need no legacy template catalog or historical environment. Shared member, schema
and family checks live in `members` and `definitions`.

`current_build.populate()` projects the verified definitions and current targets
into the shared current DB schema. `populate_field()` checks each face/printing
field before writing contexts, uses, bindings and selections. `current_render`
requires the whole field, appends anchored reminders once, preserves layout,
and returns fallback issues instead of a partial translation when any fragment or
reference label is missing. It returns exact reference positions and emphasis;
low confidence propagates through actual template and label dependencies. Named
variants are selected only by explicit pins. `render-v2` identity excludes notes,
receipts and the representative source locator. Unofficial card text does not gain
an official authority from an individual official reference label.

The command/export consumers must select this current schema and synchronize
public origin, confidence and emphasis fields before enabling current output. Frozen raw inputs for CI must come from the project's
private testdata input, never a maintainer-only path or a live website.
