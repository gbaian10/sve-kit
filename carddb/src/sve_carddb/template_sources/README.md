# Legacy JP template checkpoint

This offline tool implements translation-contract §3's legacy classification
recipe. It produces hash-only **candidate** source inventories and verifies old
fingerprints. It does not import templates, adopt reminder classifications,
infer slot types, create database tables, render translations, update an authored
index, or connect to a build/preview application.

```bash
uv --offline --directory carddb run --locked python -m sve_carddb.template_sources \
  --store /read-only/archive --store-id STORE_ID --batch-id sha256:BATCH_HEX \
  --repository /checkout --code-revision FULL_40_HEX \
  --legacy /private/templates.jsonl --expected-templates 3669 \
  --output /private/new-checkpoint
```

All inputs are explicit. `FrozenSources` verifies the sealed inventory, its
manifest **snapshot**, every descriptor/receipt/raw and their membership. Only
an exclusively JP-card batch is accepted. No latest cache, live manifest, crawler,
settings default, or network request is used. Historical raw closure is verified
but only the sealed `current` selection is counted for frequency. History gaps
are reported separately; a successful checkpoint does not claim all historical
versions are present.

The JP JSON projection and RFC 6901 pointers reuse `translations.sources`.
`translation-jp-v1` pins that parser; `classification-jp-v0-v1` pins the
normalizer. Both recipes have §3's exact six fields. The normalizer config pins
the Python/Unicode versions and the complete first-party Python dependency
closure plus `uv.lock` and `pyproject.toml`. Immutable Git blobs must equal the
loaded runtime bytes. Generated Hatch `_version.py` is excluded because it is
metadata. Symlinked modules, missing Git objects, changed implementations,
config hashes, dependency hashes, or Python/Unicode versions fail. Historical
code is verified, never dynamically executed. A runtime that no longer matches
an old pin requires the corresponding checkout, rather than silently running
the current implementation.

The v0 recipe preserves the original order:

1. Split on LF, including empty lines; in sections only, recognize the original
   token-header pattern **before** trimming. A matched definition owns subsequent
   lines until another header. Unknown header-shaped text fails.
2. Trim each remaining line, move nonnested fullwidth parenthesis matches out,
   and trim the joined body again. Empty bodies do not receive legacy IDs.
3. NFKC, replace `『…』` by `『X』`, then replace digit runs by `N`.
4. `T` plus the first 10 hex digits of SHA-256(normalized UTF-8).

This reproduces a mechanical grouping only. It does not authorize deleting
parenthesized rules, treating literal N/X as slots, assigning reference meanings,
or declaring token/reminder text translated. Moved fragments and layout receive
hash-only inventory candidates, without legacy fingerprints. Their candidate
normalization is exact source text; only body candidates use the v0 transforms.
These are not new sentence-template IDs or adopted definitions. Raw segments
use original Unicode code points, including CRLF, whitespace and noncontiguous
bodies. Their partition/UTF-8 roundtrip is independent of NFKC; normalized slot
alignment and the semantic classification required by §4 remain later work.

`checkpoint.json` has three independent results:

- `fingerprints`: complete normalized bytes must match every old T ID. Short
  ID/full hash collisions with different bytes fail. Extra new IDs are reported.
- `source_coverage`: every sealed current page, every projected face's main text
  and section, and every source code point must be accounted for. It enumerates
  fields independently from the template groups. Existing effect-presence
  evidence proves absence; unknown presence and parse failures fail coverage.
  Exact empty strings and proven absence have separate states.
  `trace_complete` reports field/byte accounting separately from the additional
  full-page presence guard. Unknown presence can fail the latter even when a
  nonempty field was transcribed; this tool does not widen the existing detector.
- `legacy_member_coverage`: every old use must match, even when another use of
  the same template already reproduced its fingerprint. Missing and changed
  uses are listed individually by member hash, without token names.

The coverage boundary is the complete parser's **ability-field projection**,
not all HTML, card names, flavor text, metadata or every historical page. A
successful fingerprint check cannot substitute for either coverage check.
Exit 0 requires all three; exit 1 writes the failed checkpoint and its per-ID/use
reasons; exit 2 indicates invalid immutable inputs/recipe/catalog. CLI errors
are redacted because lower-level parsers may include source text.

`template-sources/*.yaml` follows §3's inventory envelope and entry fields.
Each entry ID is `inv:` plus a complete canonical hash of the source reference,
line ordinal, role and original segments; enumeration of the same complete
field reconstructs it deterministically. `replay()` checks all entry fields and
the exact full-field hash against an already verified parser document; its
caller must verify the archive and recipe pins first, as `scan_batch()` does.
`source-coverage.jsonl` retains hash-only page/field/segment proofs. YAML shards
include their recipe envelopes in size measurements, target 512 KiB and stay
strictly below 1 MiB. The report maps their canonical hashes; this is **not** an
authored index or adoption receipt. Outputs are published as a fresh directory
outside the repository/archive/legacy input, never overwrite prior evidence,
and contain no official source or normalized strings.

The legacy catalog is private comparison input only. Generating source
inventories does not consult it. Real archived reproduction is a manual offline
checkpoint, not a CI fixture; tests use exclusively invented card text and
explicitly isolated synthetic Git authors.
