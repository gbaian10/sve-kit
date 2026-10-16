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
for snapshot RegionView construction, without changing printing URLs or owners.
Confirmed same-card/same-region overrides take priority. Remaining printings sort
by normalized vocabulary rarity (`registry.rarity.RARITY_ORDER`), then the earliest
known exact-day product release of their permanent `printing.home_set_id` in that
region, then raw card number and stable printing ID. Reprint inclusion dates and
`card.home_set_id` do not change this ordering.

Known rarity codes sort before unknown/uncoded rarity. The method is
`lowest_rarity` when every candidate has a ranked code,
`candidate_lowest_rarity` when an unknown competitor remains, or `fallback` when
all candidates have unknown rarity. Missing home release dates sort after known
dates; month/year precision is never promoted to a day. These methods identify
selection policy, without claiming ordinary processing or complete release dates.
Premium, signed, frame and stamp facts do not add selection criteria. The caller
supplies the intended publication subset as build rows.

The adopted override envelopes and their freshness checks remain defined by
[catalog-route adoption](../../../../../docs/schema/domains/catalog-route-adoption.md).
This selector does not implement the override adoption loader or enable complete
routes, products or release capabilities.

Run the independent synthetic counterexamples with:

```bash
uv --directory carddb run pytest tests/domains/routes/test_routes.py tests/domains/routes/test_route_defaults.py tests/domains/routes/test_routes_defensive.py
```

Official card text is not stored in these tests. Frozen production validation
uses the read-only archive and reports identifier/count metadata separately from
synthetic data.
