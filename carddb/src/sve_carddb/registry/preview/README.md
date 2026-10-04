# Regional identity staging

`plan_preview(authored, provider, regions=("jp",))` first loads and validates the
**complete** registry with `load_registry`. The region argument is mandatory.
The returned immutable plan retains all indexed shards, full decisions, members,
sample IDs, original permanent allocations and both high-water cursors. It does
not write authored data or narrow decision membership. Only subsequent build
rows are selected by region.

`FrozenJP(store, store_id, batch_id, parser_version=...)` verifies the sealed batch
and its metadata/raw/immutable-manifest closure. It reads only pinned current
entries in that batch, rechecking descriptor, first receipt and blob hashes on
access. It never opens a live manifest, accesses latest cache or downloads data.
JP extraction uses `extract.official_jp` and the existing `legacy_projection` for
`registry-observation-v1`; region, exact card number, recipe, observation hash
and rules hash must all match. Original source face indices select face metadata.
An EN suffix is never used to infer an identity or source.

Source IDs, URLs and raw hashes come from the descriptor. The locator is
`<store-id>:<relative-content-addressed-path>`, with no machine path. Fetched time,
ETag and Last-Modified come from the descriptor's **first** receipt, not the
batch's most recent receipt. Fetched time uses that receipt's `last_changed_at`,
normalized to UTC `Z`: the writer records when these raw bytes replaced the
previous content. `first_fetched_at` belongs to the URL, not this content version;
neither that timestamp nor archive observation time is substituted. Callers pin
the parser's code/dependency version explicitly.

`EvidenceProvider` is the injection boundary for already verified, pinned inputs.
Providers must bind extracted observations and face metadata to the actual raw
source version; they must not echo expected registry hashes. `CardEvidence.from_card`
computes the original recipe from extracted data. Synthetic JP/EN test providers
live in the test fixtures, outside the production package. `FrozenEN` uses the same sealed metadata reader
with the production EN extractor and its measured legacy projection.
`FrozenRegions(jp=jp_provider, en=en_provider)` composes pinned regional providers
by explicit region; it never infers counterparts from suffixes. EN exact page
number, region, descriptor kind, HTML media type and both observation hashes are
checked. Face rarity/credits come from the original source face index.
See [offline extraction](../../extract/README.md) for the EN rendering contract.
Neither provider reads historical JSONL or uses it as source evidence.
`coverage(hash)` means the **complete historic review input** with that exact hash
has been independently pinned and verified. A batch of individual HTML sources
is not that old JSONL input; `FrozenJP.coverage` therefore returns false.

## Decisions and projection diagnostics

`plan.report()` contains record keys, historic decision IDs/states, selected
regions, original cursors, missing/mismatching source diagnostics, source IDs,
URLs and hashes. It contains neither card text nor correction values. Every
registry record has one included/excluded/deferred result. The report and plan
must accompany staging output; SQL's historic `decision.state=confirmed` alone
is **not** a fresh-adoption claim.

- JP cards can retain their historic identity while unavailable EN evidence is
  reported. EN printing adoption requires its exact reviewed JP target source
  when one exists. Confirmed-none additionally requires the pinned review scope.
- Art requires matching evidence for all reviewed uses and at least one included
  use. This importer handles the registry's unclassified groups; base/alternate
  groups remain excluded with `art_baseline_deferred` until baseline evidence is
  supported. It never creates an art for unknown grouping.
- Reskins require both endpoints to have included printings in the same output
  region, and matching evidence for **every** reviewed printing in that region.
  `Projection.regions` records eligible regions. A later exporter must use that
  eligibility, rather than treating the regionless SQL relationship as valid in
  every region. Synthetic tests exercise EN; a JP-only graph cannot reference
  an excluded EN-only endpoint.
- All source corrections and their decisions remain in `plan.snapshot` without
  application, retirement or deletion. Their report status is deferred, including
  needs-review candidates. This staging API does not call `project_corrections`.

## Database boundary and failure behavior

Use `import_preview(db, plan, authored_revision=<full Git SHA>, build=build_context)` for an existing
new build database, or `populate_preview` inside the transaction owned by
`rebuild_database`. The caller supplies a stable, verified authored checkout and
its full revision. Shard source IDs pin revision, path and the canonical envelope
hash (`registry-envelope-v1`); `sha256` is the existing index's canonical JSON
content hash, not YAML serialization bytes. Every original shard and decision is
recorded, including excluded regions and deferred corrections. A decision's
`registry_envelope` source retains access to its full members, not just imported
rows. Available raw evidence has separate matched/mismatch decision-source roles.

Before writing rows, the importer requires existing verified `product_family`
parents for every selected card/printing. Product/vocabulary import is a caller
responsibility. The [product family importer](../../products/README.md) supplies
confirmed authored families and composes with `populate_preview` in the caller's
transaction. Missing parents fail explicitly; no inferred product names, types,
dates or placeholders are created. Rarity text is preserved; its normalized code
remains NULL for the product/vocabulary stage. Premium is unknown except the
specified exact pure-premium label. Conflicting rarity evidence between faces
fails for review instead of choosing a value. Printed text/current/errata are
not built here: text references are NULL and printed state is unknown. Raw face
credits are retained, but no artist grouping is inferred.

`import_preview` owns **one** transaction for sources, full historic decisions,
links, selected identities, printings, face uses and permanent integer IDs.
Source-version metadata collisions, conflicting art uses, invalid rows, missing
DDL or other errors propagate. At the end of the body, the existing typed DB
boundary runs FK/integrity/schema/cross-table verification **before COMMIT**.
Any unhandled body, verification or commit error rolls back the whole import;
previously committed caller-supplied parents remain intact. `populate_preview`
requires an existing transaction and has the same rollback contract through its
caller; it does not catch errors. The rebuild API retains the old destination
on failure.

Compile T0 for JP identities. Enable `art`, `en` and `related` DDL as required by
included review rows. This is an identity staging importer, not the complete
preview pipeline, public snapshot exporter or release gate. It does not mark
T0 or any other broad capability importer/validator ready. The later preview
pipeline still needs product, text, public projection and capability validation.

## Shared sources and build input records

`Source` lives in `sve_carddb.build_inputs`. It retains each use's parser and
sealed archive pin (store, batch, descriptor and first receipt); `Source.values()`
projects shared raw metadata with `parser_version=NULL`. Authored envelope
sources keep `registry-envelope-v1`. No source ID is derived from a parser.
`FrozenJP` and product evidence use the same `FrozenSources` metadata reader.

Both import and populate require an explicit `BuildContext` and return an
`InputRecord`. `plan.source_uses()` independently declares every successfully
read observation, including evidence used to exclude a printing. Import checks
the entire raw source/use closure; populate checks its subset for later
composition. Preserve the returned record and save completed staging DBs via
[build bundles](../../build_db/README.md#saved-build-inputs); a bare SQLite file
does not attest the build-input contract. Parser pins remain distinct from
identity decisions and never bypass the existing adoption gates.
