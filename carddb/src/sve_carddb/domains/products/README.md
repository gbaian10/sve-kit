# Product inputs and frozen official staging

`load_products(authored_root, registry=...)` reads every shard under the independent
`products/` directory defined in [authored-layout §10](../../../../../docs/schema/authored-layout.md#10-歸檔類別商品與實際收錄).
It never writes authored data, allocates IDs, reads latest cache or opens a live
manifest. Supply a stable, verified authored checkout and the complete validated
identity registry from that checkout.

The reader checks all family, product and inclusion shards before any
projection: strict YAML, complete field sets, format versions, safe exact paths,
immutable typed records, data primary keys, per-shard kind/filing key, sorted
records, global uniqueness and each record's `proposed`/`confirmed` state.
Unexpected YAML paths fail instead of being skipped. Family references,
product/printing references and inclusion region equality are checked globally,
including candidates and EN records. Filing keys do not create relationships.
Unknown/month/year dates keep their original text and never gain a guessed day.

`ProductSnapshot` retains complete canonical shard bytes, hashes, full typed
shards and the identity index used for reference checks. The record mapping is
read-only; nested models and arrays are immutable. `report()` contains
identifiers and record states only. It reports proposed inputs as candidates and
confirmed product/inclusion inputs as unimplemented projections. Loading attests authored consistency, not the
meaning of a locator, raw evidence closure or release eligibility.

## Family projection

`populate_families` runs inside a caller-owned build transaction. It records each
complete shard as an authored `source_record`, pinned by full Git SHA, path and
canonical hash. A record's optional `note` keeps the maintainer's rationale in
the authored shard; it is not copied into the DB.

Only confirmed families become `product_family` rows. Proposed family records
remain in the snapshot and report, without creating parent rows. Any manual
product or inclusion record, including a proposed one, causes an explicit
unsupported-projection error before writes. Manual product/inclusion projection
remains separate work. Frozen official
observations use the independent product identity boundary described below.

Supply `languages` using your build's explicit `Language` configuration, or
register those languages in the caller's transaction first. Identical existing
configurations are reused; conflicting configurations fail. Additional registered
languages are supported. All family name languages must be registered, including
candidate names; no fallback configuration is inferred from the name language.
Names use the build DB's exact UTF-8 text hash and
`t:{lang}:{first-16-hex}` ID recipe. Existing hash/ID matches must also match exact
text bytes and the full hash; conflicts fail instead of replacing text or
lengthening keys. This only checks the current build, not the future release
validator's permanent published-text union.

Nonempty family evidence requires explicit `stores={store_id: path}`. Projection
verifies each sealed batch and its complete manifest/descriptor/receipt/raw
closure, then resolves the referenced source version from that batch. Raw source
IDs, URL, bytes hash and locator come from archived metadata; fetched time and
HTTP metadata use the descriptor's first receipt. No URL or hash-only shortcut is
accepted. Every authored evidence reference becomes a
`product_evidence_closure` use with its canonical reference as locator; the
original evidence role and batch stay in the shard. Metadata conflicts fail. Locators
remain human evidence descriptions, never executable queries or automatic
proof of a real product relationship.

## Composition with identity preview

```python
from sve_carddb.domains.products import import_product_preview, load_products

catalog = load_products(authored_root, registry=plan.snapshot)
inputs = import_product_preview(
    db,
    catalog,
    plan,
    authored_revision=authored_revision,
    build=build_context,
    languages=build_languages,
)
```

`plan` is the existing `registry.preview.plan_preview` result. The catalog and
plan must use the same complete identity index. `import_product_preview` owns one
transaction for family, text, provenance and identity writes. Use
`populate_product_preview` inside an existing transaction or `rebuild_database`
callback; it does not start a nested transaction. The existing
`registry.preview.populate_preview` still rejects any missing family parent.
Missing or proposed parents, source metadata conflicts, missing DDL, insert
failures and commit-time FK/integrity/query failures roll back the whole combined
graph. Rebuild failures retain the old destination.

Permanent card/printing IDs, owner IDs, both registry allocation cursors and all
printing integers remain unchanged. The default supplies family parents for
identity staging. An optional validated
`official` plan also stages official products and inclusions in that transaction.
This does not build card text, vocabulary, public snapshots or a complete release
pipeline, and does not enable broad capability readiness flags.

## Shared raw evidence and saved build inputs

Raw source rows have `parser_version=NULL` and are shared with identity staging
only when every version metadata field matches. The authored source rows keep
their input profiles: `product-authored-v3` for product shards and
`product-identity-v3` for identity shards. These profiles identify both the
accepted envelope and the core canonical recipe; both documents use integer `format: 1` with
`kind: product_shard` and `kind: product_identity_shard` respectively. `FrozenSources` provides one reusable
sealed-source metadata boundary; product evidence records `archive-closure-v1`
under `product_evidence_closure`, since checking bytes does not parse or adopt
a product relationship. Identity evidence keeps its actual parser pin.

Each staging entry point requires a `BuildContext` and returns an `InputRecord`
summarizing actual source uses. Save the already completed database with
`build.output.save` together with its inputs and report. The helper uses SQLite
backup and a temporary directory with no-overwrite rename; it does not populate
a second database. See the [build DB example](../../build/README.md#saved-build-inputs).

## Confirmed permanent official product identities

`load_product_identities(authored_root, authored_revision=..., catalog=...,
stores=...)` implements [authored-layout §11](../../../../../docs/schema/authored-layout.md#11-官方商品身分對照-product-identity-v1).
It validates every shard under `products/identities/` and both regions before
projection: closed fields, strict YAML, expected file paths, path/filing/region
agreement and unique complete match keys. Every record is a confirmed
identity; the directory must exist, so a missing one never reads as empty. IDs
share the manual product namespace and cannot cross regions. Different match
aliases may share one ID; duplicate match records are forbidden even for one ID.

The loader reads current working-tree files once, validates unique match keys,
size, types and references, then sorts. `identities.configuration()` supplies
`product_identity` configuration with a provenance revision. Uncommitted edits
apply; no Git or dependency bytes are compared.

Every evidence reference resolves through the sealed archive boundary. For
`product_identity_match`, the canonical block ordinal must reproduce the complete
match and region from actual raw. Other roles only attest archive closure and
keep their human locator. Same-region expansion codes pointing to different IDs
produce a warning with all matches and source references, including outside the
selected output region; same-ID aliases do not warn.

## Official blocks, planning and import

`FrozenProducts(store, store_id, batch_id, region=...).pages()` verifies a sealed
batch and reads every current card source. It never opens live manifests or
latest cache. `parse_products` checks raw bytes against their hash, the actual
parser pin, region, exact page URL/card number, HTML media classification and
recognized block layout. `official-product-block-v1` enumerates
`.cardlist-Under .cardlist-Detail_Products_Inner` in document order. It preserves
name/date text, original parsed href attributes, resolved URLs and exact ordinal
locators; HTML entities are decoded before resolving relative links. Raw lexical
HTML remains available through that source version and locator.

Only regional HTTPS product links under `/products/` and the specified card
search paths count as match clues. Query ordering, trailing slashes, case and
compound expansion codes remain intact. Expansion parameters are decoded once;
missing, empty and repeated values remain distinct. Empty/repeated parameters or
multiple different product URLs/codes permit only a confirmed exact
`source_block` match. Dev/foreign links remain diagnostic evidence. The extractor
does not visit linked product pages or assert their contents.

`plan_official_products(identities, pages, preview)` resolves every block by all
exact confirmed aliases. Zero matches retain source/block/clue diagnostics and
exclude dependent product/inclusion rows. Multiple permanent targets fail.
Conflicting observed names or dates fail with both source locations; conflicting
other product fields also fail. Identical content deduplicates deterministically
while all distinct source uses remain. No ID is allocated from content or order.
Names/dates can change without changing a link-based permanent identity; a
source-block identity remains pinned to its exact raw version. New URLs need a
separately confirmed alias to the existing ID.

Product type follows the approved [build DB rule](../../../../../docs/schema/build-db.md#32-商品與發行).
An exact, case-sensitive match between the block's complete expansion code and
one confirmed family's `public_code` supplies that family's `kind`; `code` is
not a fallback and compound codes are not split. Missing, ambiguous or unmatched
codes retain the product with `product_type=NULL` and a `product_type_unknown`
diagnostic. Proposed families never supply types. Titles, owners and card numbers
are not type evidence. This lookup establishes no family relationship:
`family_id` remains NULL. Official inclusions always use `other`, without
claiming packs, boxes, prizes, campaigns or redemption. Unknown dates remain
unknown; month/year retain raw text with no complete day. The official
`ProductData` permits a null type; `AuthoredProductData` in §10 input requires a
non-null Code. The composed import must use the same catalog that supplied the
official type lookup.

Products may exist without an eligible printing. Inclusions require the exact
same-region card number, an included printing in the supplied identity preview
and matching source metadata. Missing/mismatched EN observations remain excluded
by that existing gate. Use `FrozenRegions(jp=FrozenJP(...), en=FrozenEN(...))` from
[regional identity staging](../registry/preview/README.md) to supply the actual
sealed JP and EN identity evidence, alongside both `FrozenProducts` batches.
Product identity confirmation does not adopt a printing. Reports distinguish
nonblocking diagnostics, including null types, from actual exclusions.

```python
identities = load_product_identities(
    authored_root,
    authored_revision=authored_revision,
    catalog=catalog,
    stores=stores,
)
official = plan_official_products(identities, frozen_pages, plan)
context = BuildContext.from_inputs(
    program_revision,
    configuration | {"product_identity": identities.configuration()},
)


def populate(db):
    return populate_product_preview(
        db,
        catalog,
        plan,
        authored_revision=authored_revision,
        build=context,
        languages=languages,
        stores=stores,
        official=official,
    )
```

Call this population inside the build transaction, then save that database.
The single transaction includes families, the existing identity graph, authored
identity shard sources and official products/inclusions. Official content
references raw sources, never an identity mapping as a content approval. Authored identity sources use `product-identity-v3`; raw parsers remain
NULL. Exact evidence closure uses `product_identity_evidence_closure` with
`archive-closure-v1`; reproduced matches use `official_product_identity` and the
actual product parser. Page scans, product and inclusion processing each retain
their own usage, including zero matches and printing-gate exclusions. Source reads verify archived bytes and metadata; owner and reference checks
apply during planning and population. Save the completed DB with inputs/report.
