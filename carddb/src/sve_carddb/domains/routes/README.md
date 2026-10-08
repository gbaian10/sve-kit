# Stable printing entries

`populate_routes(db)` runs inside the caller's transaction after identity rows and
permanent allocations exist. The regional identity importer invokes it before
committing. It derives official entries from exact confirmed `card_no` values
and provisional entries from permanent `card_int_id`, and validates the resulting
index. It rejects cross-region exact collisions, reserved/invalid keys, missing
allocations, ambiguous same-number variants without a confirmed exact-number
`route_override`, unconfirmed overrides/aliases, alias chains and canonical
hijacking. Existing entries cannot be silently replaced or removed.

`build_index(db)` verifies canonical entries against the printing rows and exposes
exact lookup, adopted aliases and NFKC + casefold lookup in that order. An ambiguous
folded key returns `ambiguous`; exact entries remain usable. `populate_routes`
records each such key in `build_issue` with severity `warning`, idempotently.
Aliases with the same canonical target do not create artificial ambiguity.
For same-number variants, only the selected printing owns that official entry;
the other variants have no separate route. No alternative variant URL is invented.
A `redirect` resolution supplies the canonical path for a consumer's HTTP 301.
The consumer must handle `reserved`, `missing` and `ambiguous` explicitly.

The codec retains raw UTF-8, including whitespace, combining characters, circled
S and lowercase a. It performs one percent decode, rejects malformed escapes,
invalid UTF-8, slash and NUL, and encodes only ASCII unreserved characters without
escapes. Paths do not use printing IDs, interface language or a slug for lookup;
an optional trailing slug is ignored. Reserved app routes take priority. A
provisional display number never claims an official entry or participates in the
official folded index.

This package validates supplied existing aliases but does not create repair
history, renumbering transitions or provisional-to-official adoption. Those
belong to the identity-repair contract. The importer currently creates the basic
entries only; it does not claim the first-publication compatibility gate.

## Default printing

`select_defaults(db)` returns typed `(card_id, region, printing_id, method)` rows
for snapshot RegionView construction, without changing any printing URL or
owner. Confirmed same-card/same-region overrides take priority. For each remaining
group, general candidates in the card's `home_set_id` take priority, then the
minimum known exact-day inclusion date, then stable printing ID. When there is no
general candidate, all display rows in that group use known-date-first ordering
and the stable ID, with `method=fallback`.

`GeneralEvidence` supplies explicit classification facts for a printing. No
rarity, frame or stamp code list is invented here. Without a classifier's facts,
the default is fallback. Callers must provide adopted classification evidence
from their pinned inputs before using this optional argument in a real build;
bare facts are not an authored loader, review envelope or release gate.
Known premium, signed, stamped or special-frame candidates are excluded from the
general pool. Unknown processing can remain a candidate but cannot prove it is
ordinary. `earliest_general` requires complete inclusion dates and ordinary
processing for the general pool, and no unclassified eligible competitor;
otherwise the method is `candidate_general`.

Each effective inclusion date is its override, or the product date when
`printing_product.first_available_precision` is NULL (no override). Only day
precision supplies a sortable date. An explicit month/year/unknown override
blocks inheritance even when the product has a full date; a non-day product date
also remains unknown. Dates are checked against the product's region, and no
first-of-month date is invented. Card-number order and another region's date do
not substitute for evidence. No inclusion rows means unknown dates even when
unrelated dated products exist. The caller supplies the intended display/publication
subset as build rows; this module does not infer a separate publication policy.
The inheritance rule follows build-db §3.2 and snapshot-format §8.

[Catalog-route adoption §7](../../../../../docs/schema/catalog-route-adoption.md#7-預設版次與一般版分類)
defines the approved general-rarity policy and the evidence required for ordinary
processing. The classifier supplies that policy from pinned authored/configuration
inputs; this package does not hardcode its whitelist. Unknown frame or stamp
evidence remains unknown. Known general rarity with incomplete processing can
yield `candidate_general`; missing general-rarity evidence yields `fallback`.
The contract also defines adopted route/default override envelopes in §2–§5.
Their loaders and freshness checks must validate those inputs before supplying DB
overrides; accepting supplied rows here does not implement the adoption loader.
Passing route validation and this selector does not enable the complete routes,
products or release capabilities; their readiness flags remain unchanged.

Run the independent synthetic counterexamples with:

```bash
uv --directory carddb run pytest tests/test_routes.py tests/test_route_defaults.py tests/test_routes_defensive.py
```

Official card text is not stored in these tests. Frozen production validation
uses the read-only archive and reports identifier/count metadata separately from
synthetic data.
