# Frozen regional text and pending wording

`TextObservations(identity_plan, provider, images=...)` owns one source-to-view
composition. It reads all selected-region
printings through an independently verified `registry.preview` identity plan.
`FrozenTexts` reads only an explicitly pinned immutable archive batch; use
`RegionalTexts` to dispatch JP and EN without guessing cross-region IDs or face
ordinals. Exact source metadata and the `registry-observation-v2` observation must match
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

`populate_text_preview(db, catalog, observations, ...)` composes identity/product
and the owned text staging in a caller-owned transaction.
`observations.populate` uses an existing transaction; `observations.import_into`
owns a transaction and rolls back dependency failures. External hand-built
`TextPlan` values are no longer importer inputs. `observations.configuration`
pins the explicit vocabulary and published text history; these external inputs
and all DB parent/revision dependencies are still checked during import.
The plan is constructed once and its selection closure is not revalidated by
import, reskin/name composition or view generation. `plan.publication_identity()` preserves pending wording
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

`observations.views(db)` returns both views from its owned plan. Its `wording`
field contains sparse `WordingView` values for each pending public face/region;
its `observed` field contains each physical
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
adoptions or enable DSL capabilities.

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
publication. Pin the complete configuration with `observations.configuration`, alongside
program revision and any existing product-identity configuration.

## Completed build output

`BuildContext.from_inputs(program_revision, configuration)` retains explicit
configuration and provenance. Populate text once inside the combined build
transaction, then save that completed DB and its actual input record/report with
`build.output.save`. Source reads verify archived raw hashes and metadata; domain
planning verifies current source ownership and observation applicability. All adapters in an offline command share `FrozenSources` through one
`FrozenBatches`; full batch closure is checked once, while each source read
rechecks descriptor, first receipt and raw hashes. Reader sharing ends with the
command. There
is no separate expected-use comparison, build seal or second DB population.

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
original projected physical revision as `supersedes_id`, and
`correction_application` names the correction. Presence, correction comparison and image evidence uses all
remain in F1. Correction reports distinguish `raw_face_hash` from
`projected_face_hash`; neither a type correction nor its hash may erase the
verified empty effect. Unknown effects still cannot acquire invented revisions.
