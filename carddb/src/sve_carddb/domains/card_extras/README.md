# Frozen Q&A, errata and related cards

`card_extras` supplies immutable `CardPage` / `ErrataPage` input records for the
initial sealed-source import and incremental adapters. Its parsing/planning and
frozen readers do not fetch pages, open a live manifest or read latest cache.
Protected fetching is a separate explicit adapter described below; none writes
authored data.

`FrozenCardExtras(store_root, store_id, batch_id).pages()` streams only current
JP card pages in the pinned batch. Every read verifies the descriptor, receipt,
raw hash, region, resource kind and physical card number. Its parser recipe is
`official-card-extras-jp-v1`. The adapter transcribes Q&A text with the existing
JP renderer, retains the original related href and resolves errata references.
`parse_card_page(..., region="en")` uses the same typed input contract with the
existing EN physical-page validator and EN text renderer. Its parser recipe is
`official-card-extras-en-v1`. Regional URLs and raw numbers remain independent.

Numbered Q&A is identified by `(region, official_number)`, regardless of its
page-local key. Unnumbered Q&A keeps `official_number=None` and uses the explicit
stable source key. The JP adapter prefers an HTML anchor; without one, it uses the
page URL and block locator. That locator is an internal source identity, not an
official Q number. Future adapters must preserve known unnumbered keys; a moved
unanchored block needs explicit identity reconciliation rather than a guessed
number. Unknown dates remain raw with parsed dates null.

Q&A revisions follow observation order (`fetched_at`, immutable source ID,
block locator), not publication date. Adjacent identical contents share a
version and union their card IDs; a same-day wording change or reversion creates
another immutable version. A change to parsed `published_on` or `updated_on`, or
to withdrawal state, also creates a version. By the coordinator's decision,
observed official date changes must remain visible in public versions rather
than leaving their dates stale. A different `date_raw` spelling with unchanged
parsed dates does not create a version; that version retains the first raw
spelling, and every observation remains in the pinned inputs and raw sources.
This order records observed versions, not a claimed official effective date.
All page/block uses survive in `InputRecord`, including
sources of deduplicated versions and pages with unknown adopted identities.

Incremental adapters must retain each numbered Q&A's page/block observations:
the same Q number on different pages can have different wording during one
crawl. Observation order then produces separate observed versions, even within
one crawl; it does not prove an official revision or select authoritative current
wording. Compare cross-page wording and inspect all retained source uses before
consuming a version as current. Do not collapse a crawl to the last page, infer
official chronology from fetch time, or discard conflicting pages. `FrozenOfficialExtras.report(root)` exposes such conflicts for reconciliation,
scoped to the latest generation rather than mixing historical changes with a
single crawl. It does not supply an authoritative-current-wording policy.

`plan_card_extras(db, pages, errata=...)` resolves related targets solely by exact
`(region, raw card_no)` in the already adopted printing graph. A resolved link
keeps the target printing ID and `official_unspecified`; it does not infer token
production, counts or DSL. Unknown, external and same-card links remain staging
entries in `plan.report()` and `build_issue`, without dangling FK or self-links.
The report contains identifiers, URLs, counts and diagnostics, never Q&A text.

`ErrataPage` accepts independently transcribed announcement/effective dates,
exact fragments and explicit face identities. Its official source must match the
region and URL. The importer validates the change's printing/face scope and the
shared correction-value schema. It leaves before/after revision references null
and never manufactures a full face revision from a fragment. A printing listing
stays `listed`; `confirmed_applies` requires an existing confirmed decision linked
to the exact announcement source. Importing an announcement does not itself
adjudicate current text. Missing printing/face evidence fails the transaction.

Compose the importer after identity and language staging, in the same caller-owned
transaction:

```python
from sve_carddb.core.provenance import BuildContext, input_record
from sve_carddb.domains.card_extras import plan_card_extras, populate_card_extras

plan = plan_card_extras(db, pinned_pages, errata=pinned_announcements)
configuration = {**other_configuration, "card_extras": plan.configuration()}
context = BuildContext.from_inputs(program_revision, configuration)
extra_inputs = populate_card_extras(db, plan, build=context)
combined = input_record(context, (*other_inputs.uses, *extra_inputs.uses))
save(db, new_build_directory, combined.content(), report)
```

The configuration pins complete input semantics by hash. Population reconstructs
the plan against the actual DB before writes and verifies its own input-use subset.
Use the existing build-bundle publication/verification boundary to recheck archived
bytes and save DB, complete input record and report together. No capability is
marked ready merely because this importer or its DDL exists.

A card-page errata reference, or an announcement listing/changing a card, records
`card_extras:errata_current_pending`. Until separate adjudication has supplied
valid current evidence, `require_card_extras_ready(db, scope)` returns typed
`CardExtrasRestriction` records for the proposed `(region, card_id)` scope.
Each record identifies the card, regional face IDs, reason and `build_issue` ID
(whose context retains the source and missing URL). Because an unresolved
reference does not establish which face is affected, the face list conservatively
covers that card's adopted faces in that region. An unresolved source
printing uses `(region, region + ":" + raw_card_no)` as its staging scope key.
It has `card_id=None` and an empty face list until identity is adopted.

Following [authored-layout §9.1](../../../../../docs/schema/domains/authored-layout.md#91-未採納表記的顯示與來源更正),
the default never rejects display or removes cards, printings or faces. Callers
keep readable observations visible, mark wording pending/conflicted as appropriate,
and limit affected faces to manual use. The function only reports restrictions;
it does not mark a face as confirmed current or modify display selection.
For an automatic operation or promotion to confirmed current, explicitly pass
`strict=True` with the operation's scope; unresolved evidence then raises.
Never use strict mode as a snapshot/display filter. Importing fragments or
confirming that an old printing was listed does not clear these restrictions.
Resolution belongs to
the subsequent adjudication workflow; this importer does not create a decision
or clear a restriction. The precise missing announcement URLs remain in staging.

No Q&A/errata `source_coverage` is inferred from card pages. Empty or partial
cardlist coverage does not prove QA/errata absence, and even complete cardlist
coverage cannot become QA/errata completeness. `plan.report().source_windows`
is empty; separately verified coverage remains the coverage producer's input.

Already adopted `same_rules_reskin` rows come from the existing registry importer.
After current text staging, `applicable_reskin_regions(db, text_plan,
vocabulary=...)` returns only regions with both complete endpoint printings,
matched identity evidence from the exact endpoint sources, unchanged physical
evidence and matching current rules fields, sections and memberships. It
recomputes these fields rather than trusting the revision ID alone. A changed
current or source withholds that region until the registry reskin is re-reviewed
against both endpoints. The
result is solely a display projection input; DSL, mechanics, support results and
construction identities are never inherited.

## Protected incremental adapters

`QACrawler(http, refresh_writer, region=..., run_id=...)` uses the existing
`Client` / `Crawler` and requires a `RefreshWriter` before any request. It fixes
the minimum gap at two seconds, preserves ETags and uses conditional requests.
It inherits the supplied HTTP client's browser User-Agent; configure that client
from `Settings.user_agent`, as the existing crawler's `cli.make_http(settings)`
does. This adapter supplies no separate User-Agent or per-request override. A 304 or byte-identical 200 preserves the source
version; a changed body goes through the protected old-version archival and
backup protocol before replacement. `collect(root)` discovers explicit listing
and detail links; `cards(raw_numbers)` refreshes explicitly selected regional
physical card pages and retains their related hrefs. It never fetches unknown
related targets automatically. Both checkpoint protected history even after an
interruption. These are library adapters with no execution entry point: no existing CLI or
other application code calls them, automatically or manually. Calling them
requires new code supplying an HTTP client and a configured protected writer.

The pure `sources.official_qa` parser retains every `.qa-List_Item` and its
original card links. Listings use `.qa-Pager` links with explicit `data-page`
numbers and `.qa-List` `data-page`, `data-max-page`, `data-total` declarations.
The root must expose all page URLs; the total counts listing positions, including
repeated detail associations. Full question/answer blocks may appear inline or
on detail pages. A partial wording block is rejected. Empty listings require an
explicit `.qa-Empty` marker. Dates use the existing full-year date parser, or
explicit `data-published-on` / `data-updated-on`; unparseable dates remain raw and
null. `data-state="withdrawn"` is explicit evidence; absence, a 404 or an
unfetched detail never manufactures withdrawal. Unanchored unnumbered blocks
are marked as needing identity reconciliation.

The listing reader was written against an assumed page layout, tested with
invented pages. Real official JP/EN Q&A listing and detail pages have not been
checked. Reading real pages with this assumed layout can fail with an error or
be reported as incomplete. This fails safely, rather than claiming that the
real Q&A list was read completely; the adapter is not ready for real-site use.
EN card-page tests use the existing EN icon renderer's `src` plus `alt` contract;
they do not establish real EN Q&A listing compatibility.

Before connecting real sources:

1. Obtain the user's authorization to request a small JP/EN sample of listing
   and detail pages, then archive those samples.
2. Check pagination, counts, dates and withdrawal signals against the frozen
   samples and change the reader to match the real layout. If the site does not
   supply counts or withdrawal evidence, resolve the completeness/withdrawal
   rules explicitly rather than inventing signals.
3. Add an execution entry point and operating instructions, supplying the
   existing configured HTTP client with `Settings.user_agent`.
4. Complete the #45 archive/backup prerequisites and a small rehearsal before
   running against real data.

This PR delivers protected history and observation-completeness checks. It can
reference #46, but does not finish real Q&A source integration or close that issue.

Generation closure requires every page from 1 through the explicit maximum,
matching declarations and targets, all referenced detail bodies, matching total
positions and no unrelated members. The root is fetched again before validation;
a changed root fails the attempt. This proves observed discovery closure, not an
atomic official snapshot. Cross-page wording conflicts can coexist with complete
discovery and remain visible for reconciliation. Interruptions and malformed or
incomplete discovery mark the new generation failed and retain partial sources.
A previous validated generation is not evidence that this new attempt completed.

`FrozenOfficialExtras(store_root, store_id, batch_id, region=...)` opens only a
verified frozen manifest snapshot. `qa_pages()` and `card_pages()` expose every
archived regional version, preserving replaced and withdrawn observations.
Listing sources are stored as `list`, details as `qa`, card pages as `card`;
pin the applicable archive scopes when sealing. `coverage(root)` independently
checks the latest attempt's raw hashes, frozen sources, exact edges, metadata and
closure even if its manifest flag says validated. `generation_pages(root)` gives
that attempt's verified observations. `report(root)` combines identifier-only
coverage and cross-page conflicts; it contains no official wording. None of
these methods writes a temporal QA `source_coverage` window or confirms semantic
absence from a card-list generation.

Compose the shared importer with all retained observations:

```python
plan = plan_card_extras(db, provider.card_pages(), qa_pages=provider.qa_pages())
# Use the same BuildContext / InputRecord composition shown above.
report = {**plan.report(), "qa_incremental": provider.report(root)}
```

`changes(previous_db, plan, references=...)` returns only new `qa_version` IDs
and affected downstream identifiers. It never adjudicates semantic impact or
changes a ruling, DSL document or translation. Association-only changes can list
affected downstream IDs without inventing a wording version. Adopted card associations come
from the existing DB and new plan. Ruling/DSL/translation QA dependency tables
are not implemented in the current build DB, so callers can supply an explicit
inventory of `DownstreamUse(qa_version_id, kind, id)` records from those producers.
Missing inventory returns null for those categories, never an empty list that
claims no references. Provided records must refer to versions in the previous
DB. Confirming inventory completeness belongs to those producers; this adapter
does not invent dependencies or implement their schemas.

## Offline launch composition

`FrozenCardExtras(..., region="jp"|"en")` streams the selected sealed card batch,
using the matching parser pin and rejecting opposite-region descriptors. The
snapshot [offline recipe](../../export/OFFLINE.md) composes these pages with typed
identity/text/product parents, preserves complete source uses and projects
supplemental restrictions even when an older current exists. The old JP-only
preview recipe retains its stricter ancillary-data prohibition. Announcement
HTML parsing remains a separate adapter; card-page references alone never create
formal errata or prove corrected wording.
