# Synthetic snapshot 3.0 contract fixtures

These handwritten synthetic wire and logical examples contain no official card
text or image binaries. Expected objects are independent of the exporter and
reader. `index.json` lists the shared Python and TypeScript inputs. Core schema,
reader, canonical and bucket vectors use the current 3.0 shape.

The profile is format `3.0.0`, 64 buckets, with all eight required capabilities.
JSON files are indented for review; manifest hashes and sizes describe
canonical-json-v1 bytes. Load each payload through its manifest key and path.
The golden must rejoin to `expected-logical.json` both from individual files and
from `text-all.json` plus images/programs attachments.

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
file/base references and manifest dependencies to the new hashes, update payload
path, byte length and row counts, then rebind config_ref and text_all.contains.
Preserve dependency membership: resealing must not repair a missing/extra
reference. These cases exercise the individual-file reader, so rebuilding the
text union is unnecessary. The expected rejection must follow byte and shape
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

`whole-name-native.json` uses synthetic whole names from stored source and target occurrences. Its compact wire has no name sets or field uses; the readers restore their unchanged annotation-v1 IDs and exact scalar ranges. Fragment columns are stored once in the container row descriptors.
