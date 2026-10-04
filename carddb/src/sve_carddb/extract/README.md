# Offline card extraction

`official_jp.extract_card` transcribes Japanese pages. `official_en.extract_card`
first validates the exact English page number, then transcribes every physical
face. It retains all info labels (including unknown labels), raw stat strings,
exact image `src`, credits, speech, page notices, product hints, related card
links and Q&A titles/questions/answers. Dates and Q&A titles remain source text;
this layer does not infer missing dates or resolve product identities.

EN `Face.raw_text` retains the complete rendered effect, including separator
lines. `Face.text` is the part before the first separator; `Face.sections`
retains every later section, including empty sections. Missing effect/speech
nodes are `None`; present empty nodes are empty strings. Traits retain both the
original `Trait` value and the list split at the EN ` / ` delimiter. Numbers
such as `-` or leading zeros are never coerced here.

## EN legacy compatibility

The measured historical EN renderer emits text icons as
`{filename_stem|alt}`. The stem comes from the source URL's path basename with
its last extension removed; query/fragment do not enter it. Both the original
alt and spelling/case of the stem are preserved. `<br>` becomes a newline;
block elements start a new line and horizontal whitespace is collapsed.
An icon without a nonempty src, alt or filename stem fails extraction. There
is no fallback renderer that silently drops unknown icons.

`official_en.legacy_projection(record)` independently constructs the existing
registry `Card`: name, full info, stats, **complete raw_text**, speech and image
for each face. Historical EN records kept auxiliary sections inside full text,
so the legacy `sections` list remains empty while the new record separately
exposes them. No section content is removed to make a hash match. Page hints
and credits are retained in the full record; they are outside the original
`registry-observation-v1` recipe. Neither that recipe nor old decisions change.

## Sealed extraction

```bash
SVE_DATA_DIR=/tmp/sve-test-data uv --directory carddb run sve-carddb archive extract-cards \
  /path/to/archive 'sha256:<64-hex>' /tmp/en-records.jsonl \
  --store-id example --region en
```

`--region` defaults to `jp`; only `jp` and `en` are supported. The command
verifies the sealed batch and reads its immutable manifest snapshot, never the
live manifest or latest cache. It writes derived JSONL atomically outside the
store. Missing/failed sources stay in the report, and the CLI exits nonzero.
Parse failures report exception types without echoing source text.

These records are extraction inputs, not public snapshots or new authored
truth. Use the [regional evidence providers](../registry/preview/README.md) to
verify legacy identity observations and preserve the shared raw-source/build
input contract. Matching both observation hashes does not establish cross-region
text equivalence, correction adoption, review scope or release readiness.

`acceptance_en.acceptance_report` composes the identity, product, text and
correction plans into a redacted per-printing EN report and re-review queue.
See [EN integration acceptance](acceptance_en.md) for mismatch handling and the
separate expected-input, F1 bundle and regional text review gates.
