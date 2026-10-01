# Product inputs and frozen official staging

`load_products(authored_root, registry=...)` reads the independent
`products/index.yaml` defined in [authored-layout §10](../../../../docs/schema/authored-layout.md#10-歸檔類別商品與實際收錄).
It never writes authored data, allocates IDs, reads latest cache or opens a live
manifest. Supply a stable, verified authored checkout and the complete validated
identity registry from that checkout.

The reader checks all indexed family, product and inclusion shards before any
projection: strict YAML, complete field sets, format versions, safe exact paths,
file inventory, canonical parsed hashes, immutable typed records, data primary
keys, per-shard kind/filing key, sorted records, global uniqueness, exact decision
members/hashes/IDs and confirmed/proposed review metadata. Family references,
product/printing references and inclusion region equality are checked globally,
including candidates and EN records. Filing keys do not create relationships.
Unknown/month/year dates keep their original text and never gain a guessed day.

`ProductSnapshot` retains the canonical index, complete canonical shard bytes,
hashes, full typed envelopes, original decisions and the identity index used for
reference checks. Record/decision mappings are read-only; nested models and
arrays are immutable. `report()` contains identifiers and decision states only.
It reports proposed inputs as candidates and confirmed product/inclusion inputs
as unimplemented projections. Loading attests authored consistency, not the
meaning of a locator, raw evidence closure or release eligibility.

## Family projection

`populate_families` runs inside a caller-owned build transaction. It records each
complete shard as an authored `source_record`, pinned by full Git SHA, path and
canonical hash; every decision links back to that envelope. DB decision columns
retain the supplied review time, membership hash, checked set and original note.
Review precision and full members remain accessible through the immutable
source envelope, since the DB has no columns for them.

Only confirmed families become `product_family` rows. Proposed family decisions
and their audit trail remain available, without creating parent rows. Any manual
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
accepted. Each distinct authored evidence reference has a `decision_source` link
with its locator; its role is `product_evidence:<full-reference-hash>`, with the
original evidence role and batch retained in the envelope. Shared references are
deduplicated, while different locators survive. Metadata conflicts fail. Locators
remain human evidence descriptions, never executable queries or automatic
proof of a real product relationship.

## Composition with identity preview

```python
from sve_carddb.products import import_product_preview, load_products

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
their original envelope parser recipes. `FrozenSources` provides one reusable
sealed-source metadata boundary; product evidence records `archive-closure-v1`
under `product_evidence_closure`, since checking bytes does not parse or adopt
a product relationship. Identity evidence keeps its actual parser pin.

Each staging entry point requires an explicit `BuildContext` and returns an
immutable `InputRecord`. `product_preview_uses(catalog, plan, stores)` declares
expected uses independently of writes. Import verifies the complete combined
raw source/use closure; standalone populate operations verify their own subset,
which the caller must include in a final complete record.

Save a completed staging build using `build_bundle.publish_bundle`: it owns the
transaction, checks an independently supplied context/use plan, rechecks archive
closure and publishes DB, inputs and report together. Its population callback
uses `populate_product_preview`, not the transaction-owning import function.
`verify_bundle` requires the same pinned expected inputs and named stores, checks
all four files and reads the closed DB without writes. See the
[build DB example](../build_db/README.md#saved-build-inputs) and the
[approved source contract](../../../../docs/schema/source-archive.md#221-建置輸入紀錄與完整使用閉包).

## Confirmed permanent official product identities

`load_product_identities(authored_root, authored_revision=..., catalog=...,
stores=...)` implements [authored-layout §11](../../../../docs/schema/authored-layout.md#11-官方商品身分對照-product-identity-v1).
It validates the complete independent index and both regions before projection:
closed fields, strict YAML, exact indexed file closure, canonical semantic hashes,
path/filing/region agreement, sorted unique complete match keys, confirmed-only
whole-shard decisions and independently recomputed members/checked sets. IDs
share the manual product namespace and cannot cross regions. Different match
aliases may share one ID; duplicate match records are forbidden even for one ID.

The loader checks index/shard exact bytes against the supplied full Git revision
and retains both physical dependency bytes and canonical index/shard hashes.
Use `identities.dependencies()` in `BuildContext.from_inputs` and save
`identities.configuration()` under its `product_identity` configuration key.
Import checks every dependency and all three configuration fields. The checkout
must have Git available; dirty bytes cannot claim a committed authored revision.
No input is written or signed by this loader.

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

This parser recognizes explicit package nouns in the block title: Japanese
pack/deck/card-set and English pack, named deck/set or bundle forms. These become
`pack/deck/set/bundle` product types. Explicit packs use `inclusion_kind=pack`;
observed decks/sets/bundles use `other`, without claiming a box, prize, campaign
or redemption. Unrecognized or conflicting nouns leave a missing-type/distribution
diagnostic and exclude those rows. It never derives type or distribution from a
family, owner, card number or product ID. Unknown dates stay unknown; month/year
retain raw text with no complete day. All source-only products keep
`family_id=NULL`; no cross-region family relationship is inferred.

Products may exist without an eligible printing. Inclusions require the exact
same-region card number, an included printing in the supplied identity preview
and matching source metadata. Missing/mismatched EN observations remain excluded
by that existing gate. `FrozenProducts` provides product evidence; it does not
provide or authorize a production EN identity adapter. Reports distinguish
nonblocking diagnostics from exclusions, without claiming complete catalog
coverage or printing adoption from product identity confirmation.

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
    dependency_bytes | identities.dependencies(),
    configuration | {"product_identity": identities.configuration()},
)
expected = product_preview_uses(catalog, plan, stores, official=official)


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

Pass this callback and independently declared `expected` to `publish_bundle`.
The single transaction includes families, the existing identity graph, complete
identity envelopes/decisions/evidence links and official products/inclusions.
Official content references raw sources, never an identity decision as a content
approval. Authored identity sources use `product-identity-v1`; raw parsers remain
NULL. Exact evidence closure uses `product_identity_evidence_closure` with
`archive-closure-v1`; reproduced matches use `official_product_identity` and the
actual product parser. Page scans, product and inclusion processing each retain
their own usage, including zero matches and printing-gate exclusions. Standalone
and composed input checks reject omissions; bundle verification rechecks the
actual archived bytes and complete F1 closure before saving all four artifacts.
