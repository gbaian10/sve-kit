# Synthetic snapshot 4.0 contract fixtures

These handwritten synthetic wire and logical examples contain no official card
text or image binaries. Expected objects are independent of the exporter and
reader. `index.json` lists the shared Python and TypeScript inputs. Core schema,
reader, canonical and bucket vectors use the current 4.0 shape.

The profile is format `4.0.0`, minimum reader `4.0.2`, 64 buckets, with all eight required capabilities.
JSON files are indented for review; manifest hashes and sizes describe
canonical-json-v1 bytes. Load each payload through its manifest key and path.
The golden must rejoin from individual files to `expected-logical.json`.

The golden carries a BR-style badge with printed BR on one printing and a badge explicitly absent on the other; schema cases cover all badge styles and reject missing text, unknown styles and extra fields. Official rarity remains independent.

The golden has permanent printing identities 1/2, front/back ordinals 0/1,
separate card/art versions 7/11, and two available printing bindings sharing one
image. Pending back images, missing on the first printing and unfetched on the
second, expose neither versions nor variants. Five display sizes retain their
actual dimensions. Two synthetic card-level `same_name` links are unreviewed,
with both sv1/svwb endpoints present and offline status unknown. They demonstrate
wire capability only, not adoption of any real name-policy links.

## Shared rejection procedure

`reader-invalid.json` and `reader-invalid-core.json` use `name`, `target`, `path`,
`value`, `rehash`, optional ordered `setup`, and `error`. A target is `manifest`
or a manifest file key. Integer path segments index arrays; strings index objects.
For each case, copy the golden; apply setup then the primary replacement.
Reseal payloads in config/programs/bootstrap, text, images order. Rebind embedded
file/base references and manifest dependencies/source_refs to the new hashes, update payload
path, byte length and row counts, then rebind config_ref.
Preserve dependency membership: resealing must not repair a missing/extra
reference. The expected rejection must follow byte and shape
validation, not an unrelated stale hash. `error` identifies the Python message;
TS must reject for the same reason without adopting Python exception wording.

Cases cover incomplete/unsorted sizes, decoded metadata
size disagreement, unavailable URLs, duplicate integer IDs and same-card face
ordinals, non-card/human-reviewed same_name, duplicate/shadowed pairs, wrong
endpoints, and exact media dependency closure. The ordinal case adds an unprinted
face and orders the card face list accordingly, so it reaches identity uniqueness
rather than the earlier printing-face join check.

`schema-invalid.json` and `schema-invalid-core.json` supply a `target` definition
and rejected tuple `value`. `schema-valid.json` covers current definitions.
For invalid numbers outside canonical-json-v1, `raw_json` preserves the exact
input and `error` identifies rejection at that earlier boundary. Do not coerce
booleans, strings or floats into integers before validation.

## URLs and Index negotiation

`image-url-cases.json` contains positive cases (`url`) and negative cases
(`reject`). Negative `raw_json` is the accessor argument object, preserving
invalid scalar types. Positive cases cover JP/EN identity examples, f0 omission,
f1 and a permanent f7, manual-printing identities, both groups and the safe
integer boundary. Negative cases include zero IDs/versions, negative ordinals,
unsafe integers, booleans, strings, floats and unknown sizes.

`index-cases.json` embeds each Index and its supplied `manifests` map, keyed by
hash. Canonicalize each supplied manifest to bytes before invoking the full
Index reader. Successful cases specify `selected_data_version` (null means none
compatible). Future format, unknown capabilities and higher minimum versions
must fall back without treating the Index as damaged; do not fetch the
incompatible current manifest. An `error` case checks path/hash disagreement.
Both the full Index validation and compatibility selection must be exercised.

## Producer-only checks

Python additionally tests card/art independent changes, A→B→A, restored bytes
getting a new version, burned numbers of failed exports, and post-write
WebP corruption before pointer activation. TS consumes these versions and does
not allocate them. Those allocation/fault cases remain Python-only; a TS URL
consumer still uses the positive group-version cases above.

Run the shared Python consumer from the repo root:

```bash
uv --directory carddb run pytest tests/export/test_snapshot_media_vectors.py tests/export/test_snapshot_media.py -n 4 --no-cov
```

TS consumers can use the same JSON mutation and resealing procedure; no Python
runtime, build DB or official input is required. Review changes to the wire and
independent expected objects together; never regenerate expected objects from
producer output to make a test pass.

## Native producer regression snapshot

`annotated-native.json` is a producer regression snapshot, not an independent
Python oracle. Its manifest and logical output detect changes to the native
annotation projection. The handwritten assertions in
`test_annotation_projection.py` check the intended relationships independently.
TypeScript also consumes these bytes as reader input. Regenerate this snapshot
only after reviewing a deliberate producer or wire contract change, and keep
the independent assertions and shared public annotation cases passing.

`english-native.json` is one Python-built snapshot of a synthetic EN-only face
whose effect has a selected whole-field zh-Hant translation and whose name has none.
It is TS reader input only: the translation ID embeds the sealed synthetic archive
batch, so a rebuild is not byte-identical and no Python test compares against it.
The Python build, reader and projection cases live in
`test_snapshot_offline_english.py`.

`whole-name-native.json` uses synthetic whole names from stored source and target occurrences. Its compact wire has no name sets or field uses; the readers restore their unchanged annotation-v1 IDs and exact scalar ranges. Root row columns come from the fixed format schema; containers carry only the nested tuple descriptors they use.

`source-native.json` is a synthetic producer snapshot for on-demand Japanese source
presentation and official counterparts. Its explicit assertions cover source body,
sections, independent annotation sets, one-hop file requests and corrupt pointers or
dictionaries. Its strings are authored test data; it contains no official card text.

## Language matrix

`language-matrix.json` is shared by the Python reader, TypeScript resolver and Web
component tests. Its eighteen independently specified expectations use distinct
J/E/Z/Jc/Ec strings and one synthetic native snapshot. Separate donor revisions of
the same confirmed card and face keep counterpart text different from each
receiver's original. Missing-counterpart cases still have Chinese translations,
so Japanese and English UI tests can detect an incorrect Chinese fallback.

Both low-confidence official-counterpart cases are reader-only synthetic inputs;
they do not establish producer eligibility or alignment. Rejection cases change
the selected translation's origin or authority while retaining its low-confidence
flag and valid owner pointers. Reseal the changed translation file and all exact
references before invoking the readers, so failures test eligibility rather than
stale bytes or hashes.

`mentioned-by-native.json` is a synthetic producer snapshot whose current effect
names another card. Web tests locate the target card's existing engine band and
read its `card_mentioned_by` row from the shared file already used by
`card_related`, without another request or annotation details. Independent
Python projection assertions cover self references, duplicates, withheld targets,
sections, historical versions and selected Japanese source annotations for English.

Global detail with null region uses width 4 (16 bands); regional global detail
stays at width 8. Old previews with reader minimum 4.0.0/4.0.1 are skipped by
index negotiation and must be rebuilt. The mixed window supplies only the
readable current manifest; its older previous entry is never fetched.
