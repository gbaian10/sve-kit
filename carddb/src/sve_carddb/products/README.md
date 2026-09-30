# Product authored input and family staging

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
unsupported-projection error before writes. Those projections and extraction of
frozen product observations are separate work.

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
import_product_preview(
    db,
    catalog,
    plan,
    authored_revision=authored_revision,
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
printing integers remain unchanged. This supplies family parents for identity
staging; it does not build card text, products, inclusions, vocabulary, public
snapshots or a complete release pipeline, and does not enable broad capability
readiness flags.
