# Construction evidence staging

This package projects the existing `build-db` construction and CR tables from
strict, immutable Python records or internal JSON. It does not define a new
authored YAML layout, discover official rules, or adopt any real rules data.
`load_construction` rejects duplicate keys, unknown fields, invalid calendar
dates, ambiguous limit fields and nonofficial source kinds. All source inputs
use the existing frozen `Source` / `SourceUse` boundary, including archive,
parser and locator pins. The caller must supply verified frozen observations;
this package does not read raw files or verify archive blobs itself.

Pin `staging.configuration()` in the existing `BuildContext`, then compose
`populate_construction(db, staging, build=build)` inside `db.transaction()`.
Compile the `cr` capability when supplying CR records or a nonnull CR FK.
The returned `InputRecord` retains source uses for profiles, revisions,
restrictions, coverage, CR versions and individual clauses. A larger composer
can combine these with other input records; verification permits its other raw
sources. Existing rows are reused only when all stored values match exactly.
Any failure rolls back with the caller's transaction.

Profile revision intervals are half open and cannot overlap within a profile.
Adjacent revisions are valid. Different restrictions can apply simultaneously;
their members remain grouped by `choice_option` and `deck_scope`. `copy_limit`
uses only `max_copies` (including zero), and `choice_group` uses only
`max_selected_groups`. Choice options identify groups, not a limit on total cards.
Members must use the profile's region. A referenced CR version must exist, match
that region and have concrete clauses. Clause numbers are unique within a source
version; identical public version labels with different sources remain distinct.
Official dates and source URLs are preserved without inferring missing dates.

`resolve_construction` requires explicit `on_date`, `as_of` and `supported_refs`.
The caller takes `as_of` from the pinned snapshot publication UTC date and supplies
only versioned algorithm references that its evaluator actually supports. This
package preserves the opaque `construction_rules_ref`; it has no built-in list
of accepted refs. An absent or unsupported ref, absent copy limit or revision,
partial/missing/conflicting coverage, or a date after `as_of` returns unknown
inputs. An open coverage end does not prove future completeness. Overlapping
coverage must be reconciled before import; a read of overlapping existing rows
also returns unknown rather than selecting a complete row.

Only effective `confirmed` restrictions are returned as active. Effective
`announced` restrictions leave inputs unknown; `withdrawn` restrictions do not
apply. Empty confirmed member sets and missing referenced CR clauses also leave
inputs unknown. Ready inputs are evidence for a future evaluator, not a legality
verdict: `legality` remains `unknown` because this API does not inspect a deck or
execute the pinned construction algorithm. Physical-card counting, special
layouts and the full construction evaluator belong to later work.

Decision-backed restriction adoption and `deck_role_override` import fail closed,
even when the decision is confirmed. The authored contract does not yet specify
construction adoption categories, exact reviewed membership and freshness. An
identity or catalog decision cannot authorize unrelated rule bytes. There is no
flag that bypasses this gate. The DB additionally rejects unconfirmed decisions
used by confirmed restrictions and overrides for cards without a printing in
that region; these structural checks do not prove adoption freshness. Unconfirmed
override candidates may remain in the DB, as in the existing snapshot contract;
the public projection ignores them rather than applying their proposed role.
Official restrictions with null decisions are staging observations only.

Do not feed real rules into this staging API as adopted data until the source
inventory and adoption contract have been confirmed. No real construction
profile, restriction, CR clause or role override is adopted by this change.
Printing catalog and Decklog fields are untouched: unverified availability keeps
the catalog default, and no verification date is invented.
