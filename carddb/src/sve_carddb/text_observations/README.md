# Report-only regional text observations

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

## Diagnostic staging and pending exclusion proposal

`populate_text_preview` composes the original identity/product staging and text
staging in one caller-owned transaction. Representable identity-eligible
observations remain in diagnostic DB rows even when their face-region is pending;
all actual observation and current-comparison source uses survive. This DB is a
diagnostic build, not a public snapshot.

`plan.eligible` computes a diagnostic exclusion closure using the pending proposal
in [#143](https://github.com/gbaian10/sve-kit/issues/143). This is not a publication
gate and must not be used for snapshot output before user approval. The existing
`eligible_identity` report key is retained for compatibility; its companion
`eligible_identity_diagnostic` marks `proposal="pending-#143"`,
`publication_gate=false` and `snapshot_output_authorized=false`.

Under this proposal, one unresolved required face would exclude every regional
printing of that card, its integer and physical face references. Shared identities
would remain where another region remains available. Related edges would require
both endpoints in their declared region; unused art and mapping projections would
be excluded. `exclusion_report(db, schema, plan)` diagnoses the resulting closure
over every enabled FK, including products, inclusions, routes, aliases, defaults
and optional text references. It carries the same pending-proposal labels and
reports counts and primary keys without card text. `populate_text_preview` still
uses `plan.identity`, keeping pending observations in diagnostic DB rows. Source
and decision history remain intact. No authored records, IDs or allocation cursors
are rewritten.

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
    exclusion_report,
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
    report["exclusion_closure"] = exclusion_report(db, schema, plan)
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
terminal illustrator blocks. Duplicate credits or unknown blocks remain unknown.
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
