# Frozen regional text and pending wording

`plan_text_observations(identity_plan, provider)` reads all selected-region
printings through an independently verified `registry.preview` identity plan.
`FrozenTexts` reads only an explicitly pinned immutable archive batch; use
`RegionalTexts` to dispatch JP and EN without guessing cross-region IDs or face
ordinals. Exact source metadata and the compatible legacy observation must match
identity evidence. Full source face maps bind each original index to its authored
face ID. The EN adapter retains the new extractor's main text, ordered auxiliary
sections and Universe field.

The plan compares name, main effect, ordered sections, raw class/type/stats,
traits and title exactly. Flavor stays with the physical printing. Text interning
uses exact UTF-8, language, the full SHA-256 and `t:{lang}:{first-16-hex}`; no Unicode
normalization, whitespace changes or parenthesis stripping occurs. Both existing
build rows and the explicitly supplied union of all published texts must agree
on exact bytes for every short-ID and full-hash match. Supply `published=()` only
when no texts have been published.

An initial current exists only when every observation of a face-region has one
identical current-bearing variant and there are no missing sources, identity
gates, null effects, errata links or unresolved source corrections. When an explicit
image provider is supplied, successful [source corrections](../source_corrections/README.md)
are applied to separate candidates before comparison; raw observations remain exact.
Differences stay
pending, including changes that might eventually prove equivalent. Source dates
and fetch times never rank variants. Current uses the existing
`latest_observed_no_errata` basis with no invented decision. Candidate revision
ordinals enumerate deterministically sorted revision IDs; raw variants precede
corrected variants. `initial`/`wording` records represent
observed candidates, without equivalence, temporal ordering, supersession or
human adoption claims. Effective dates stay null and temporal status unknown.
Printed effects and section kinds remain unknown; sections preserve empty strings
and original order. Numeric symbols `-`, `X` and `Q` stay null in DB numeric
columns and survive exactly in the report. Invalid numeric values fail.

`effect=None` survives in reports and both F1 source uses, with its actual
parser, usage locator and archive pin. It creates no `face_revision` or
`printing_face_observation`. A present empty string is an exact text value and
can be materialized. Reports separately count `total_observations`,
`materialized_observations`, `deferred_missing_effect_observations` and identity
exclusions. The no-effect follower hint is diagnostic: a null main effect, no
sections, an ordinary follower type and three numeric stats. It never fills text.

## Publication and diagnostic exclusions

`populate_text_preview` composes identity/product and text staging in a
caller-owned transaction. `plan.publication_identity()` preserves pending wording
and enforces the independent correction/identity integrity gates. Snapshot
projectors use this identity selection; pending wording does not delete cards,
printings, routes, defaults or integer IDs.

The retired text exclusion proposal is available only as
`plan.diagnostic_exclusions` and `diagnostic_exclusion_report(db, schema, plan)`.
Report keys are `diagnostic_exclusions` and `diagnostic_exclusion_status`, with
`proposal="retired-text-exclusion-proposal"`, `publication_gate=false` and
`snapshot_output_authorized=false`. The former `eligible` and `exclusion_report`
names have been removed so callers must explicitly select the diagnostic API.
The general `reference_exclusions` helper remains available for the independent
correction quarantine. No authored records, IDs or allocation cursors are rewritten.

## Pending public projections

`wording_views(db, plan)` returns sparse `WordingView` values for each pending
public face/region. `printing_observed_texts(db, plan)` returns each physical
printing's own observations, using successfully corrected candidates while the
original observation and correction application stay separate in the build DB.
An unknown main effect is a null revision with `missing_effect`; unavailable
text positions retain the independently pinned identity source URL. These values
never fill printed text columns or create an adopted current.

This projector currently produces only `available` and `missing_effect`.
`correction_conflict` is reserved by the public contract but is not emitted here:
active conflicts are quarantined by `publication_identity()` before projection.
A `needs_review` correction leaves the original observation `available`, while
its face/region remains blocked from provisional display; it does not produce
a per-observation conflict marker.

`printing_dates(db)` uses formal product/inclusion evidence. A null inclusion
precision inherits its product date; an explicit unknown overrides it. Every
inclusion must have a complete day before the minimum proves a printing's first
availability. `wording_views` prefers a valid current. Without one, it compares
all dated candidate printings before testing content availability. The latest day
may display one exact content (even when separate correction contexts yield
different revision IDs); same-day different content or an unavailable latest
candidate returns `basis=candidates`. Undated printings remain explicitly listed.
ID ordering only chooses a stable representative and never supplies chronology.

`wording_region_blocks(db, plan)` declares `wording_pending` for each public
card/region missing a current on any required face. `mark_wording_pending` adds
that reason and disables automatic operation on existing build support rows;
callers generating support rows later must merge the declared blocks. A settled
front and pending back both remain visible. These APIs do not consume equivalence
adoptions or enable semantics/DSL capabilities.

Format 1.0.0's candidate schema, descriptors, handwritten shared golden and Python
reader now include `WordingDisplay`, `WordingCandidate`, `WordingView`,
`ObservedText`, `face.wording`, and `PrintingFace.observations`. Display revisions
join current revisions in bootstrap; other candidates remain in history/detail.
The web reader update is reviewed separately and must land before this extension
of the shared golden. Public inclusion dates use `available_on/date_precision`;
these are distinct from the build DB columns `first_available_on/first_available_precision`.
The later complete snapshot projector must apply these values and measure the
entire dual-region bootstrap with available three-language name closure against
the 1 MiB compressed budget. This module alone is not a complete snapshot export.

Raw vocabulary bindings are an explicit caller input, not inferred translations
or a new authored format. `Vocabulary` requires exact unique regional bindings
and declared special-kind references. Missing/conflicting bindings fail before
publication. Pin the complete configuration with `text_configuration`, alongside
program/dependency pins and any existing product-identity configuration.

## Inseparable F1 bundle

```python
from sve_carddb.build_bundle import publish_bundle, verify_bundle
from sve_carddb.build_inputs import BuildContext
from sve_carddb.text_observations import (
    diagnostic_exclusion_report,
    populate_text_preview,
    text_configuration,
    text_preview_uses,
)

configuration.update(text_configuration(plan, vocabulary, published_texts))
context = BuildContext.from_inputs(program_revision, dependency_bytes, configuration)
expected = text_preview_uses(catalog, plan, stores, official=official_products)
report = plan.report()


def populate(db):
    inputs = populate_text_preview(
        db,
        catalog,
        plan,
        authored_revision=authored_revision,
        build=context,
        vocabulary=vocabulary,
        published=published_texts,
        languages=languages,
        stores=stores,
        official=official_products,
    )
    report["exclusion_closure"] = diagnostic_exclusion_report(db, schema, plan)
    return inputs


publish_bundle(schema, destination, context, expected, populate, report, stores=stores)
verify_bundle(schema, destination, context, expected, stores=stores)
```

The existing F1 container publishes `build.sqlite`, `inputs.json`, `report.json`
and `seal.json` together and verifies source/use closure independently of row
insertion. Null observations are included in the expected uses. Raw sources keep
`parser_version=NULL`; parser recipes belong to each use. Failures roll back the
whole graph and leave the destination unpublished. Redacted differences contain
hashes, byte lengths and character edit offsets, never official card wording.

This segment does not enable semantics tables, create an adoption input format,
export a public snapshot or declare release readiness. Human current adoption,
errata coverage and publication remain separate gates. Correction staging is an
explicit optional input; scoped known corrections require verified image evidence
before the importer can proceed.

## Frozen effect presence

`FrozenTexts` retains extractor faces unchanged and adds `effect-presence-v1`
results, hashed over every result field. The pinned `detail-v1` recipe requires
closed body/html boundaries, complete names, images, info, stats and an exact
physical card-number credit on every face. The omission template additionally
requires the img/txt/ttl/txt-Inner topology and intact info/status/optional speech/
terminal illustrator blocks. Duplicate credits or unknown blocks remain unknown. The JP notice template allows
one terminal extra illustrator block only when its complete serialized HTML hash
matches one of five pinned publication/errata notices. The physical card-number
credit must still be unique and precede that notice; notices never waive an errata
or correction gate. Notice text or URL changes return to unknown.
An absent `.detail` selector alone proves nothing.

Known empty containers allow only plain div/p/span/br layout nodes. Whitespace is
present text, and unknown icons, SVG, classes or styles never become empty text.
The projection converts proven absence to an exact empty string; unproven empty
values stay null. Sections and the original extractor hash remain unchanged.
`verify_card` re-extracts the frozen bytes and reproduces all presence results
before the observation plan can be imported. A self-consistent replacement hash
cannot substitute for this verification.

Each frozen face contributes a separate F1 `effect_presence` use with the actual
presence parser, complete reference locator and result hash, including deferred
faces. Reports and configuration retain complete evidence and both raw and
projected wording-face-v1 hashes. Shared empty text units carry no source evidence.
Synthetic/trusted non-HTML providers without presence results retain their explicit
input values; they cannot attach presence proofs without frozen bytes.

Presence projection precedes source correction: each physical observation keeps
its untouched extractor face on `card.faces` and its presence-projected content;
`corrected_observations` applies verified replacements to that projected content.
Only corrected candidates carry `correction_keys`. Their revisions retain the
original projected physical revision as `supersedes_id`, with the independent
correction decision. Presence, correction comparison and image evidence uses all
remain in F1. Correction reports distinguish `raw_face_hash` from
`projected_face_hash`; neither a type correction nor its hash may erase the
verified empty effect. Unknown effects still cannot acquire invented revisions.

### Explicit presence v2 matcher

`presence_v2.detect_presence` produces separate `effect-presence-v2` evidence
with parser `effect-presence-v2/detail-v2`. The v1 module and evidence types stay
unchanged. This is an opt-in matcher API: existing `FrozenTexts`, wording review
receipts, build and preview consumers continue using v1. V2 evidence is rejected
by the v1 model and cannot silently replace an old adoption's pinned observation.
A caller selecting v2 must pin this module and its v1/helper dependencies, verify
the frozen source closure, and replay the complete result rather than trusting
its self-consistent hash. No adoption record is migrated by this change.

The new JP variants require the whole HTML tail, a unique detail root, one or two
complete faces and a numbered front credit. A terminal notice must follow the
physical credit directly, have no name/heading, and match its exact serialized
HTML hash. Twenty additional frozen notice hashes are recognized; these are
source evidence, not adopted corrections or copied notice prose. Changed notice
text, links, markup or position remain unknown.

Only the second face may omit its credit or contain an artist-only heading.
It must have a nonempty recognized ability container, complete info/stats and
bounded wrapper children. Artist-only credit is terminal and contains one plain
heading of letters/spaces; a different card number cannot masquerade as an artist.
Every newly recognized variant, including notices and the front of a two-face
page, can only prove presence. A non-present container result retains the
original unknown/incomplete_source conclusion and does not acquire the new
credit template. These variants never prove additional absence. Known v1
present/absent results retain their state and reason; new recipe/parser/template
identities intentionally produce new evidence hashes. The upgrade must be
checked against every frozen JP face, including all previously proven absent
faces, before use.
