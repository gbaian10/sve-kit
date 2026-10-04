# Adopted glossary and digital names

This library implements the glossary boundary of translation-contract §2/§5.
It has no crawler, CLI command, application entry point or snapshot selection.
All source reads require explicit sealed batches, immutable Git recipe bytes and
F1 dependency/configuration pins. It never reads a research draft or latest cache.

`load_glossary()` checks the entire indexed entry and immutable decision histories.
Every non-indexed file is rejected, including temporary files. Candidates and bare
model reviews are rejected. A source value must match its declared language,
full-field exact hash and Unicode code-point span. Dictionary evidence must use
the same official key; effect evidence must locate actual effect fields. Official
names require an adopted same-card link to the exact digital and SVE faces.

`compile_build(("translation_evidence",))` creates the nine-table minimum closure
with T0 dependencies. It does not enable template, art or voice capabilities.
`import_glossary()` owns a transaction; `populate_glossary()` and `import_digital()`
compose in a caller-owned transaction. Authored index/shard bytes must match the
pinned authored commit. Historical choices and raw sources remain traceable after
withdrawal. Identical translated strings never merge different concept IDs.

## Delegated card-name preparation

`card_names.prepare()` creates glossary term/zh-Hant choice envelopes from an
explicit key/value/source-claim map and its delegated coordinator receipt. It
requires the receipt's exact record-key scope and the complete map hash in its
decision basis. This is an authoring helper; supplying a receipt is not evidence
that approval happened. Obtain the coordinator's actual key-allocation decision
before calling it. Bare model reviews and another batch's delegation do not grant
that authority.

Candidates contain a frozen JP name reference, never a separate original-name
field. The helper keeps `machine` wording as `machine`; fully borrowed wording
uses `project` with its unverified `source_claim`. Callers decide provenance per
name before preparation; draft basis/confidence is not an adoption rule. Omit
untranslated or pending candidates and preserve their IDs, hashes and confidence
in a separate private report. These envelopes create no card identity, same-card
links, per-card exceptions or emphasis overrides.

Validate all proposed keys against the complete adopted glossary first. The
helper rejects reused keys, duplicate exact names and non-JP/name locators; the
regular loader/importer must still replay every frozen field and source closure.
It returns small, deterministic, single-kind shards without writing files. Append
their sequences and update the existing index only after checking the complete
closure. Previously adopted files remain immutable.

`import_digital()` reads the explicitly pinned API inputs and only imports the
requested cards, required parent cards and all recognized name faces/languages.
Absent parents or inconsistent localized face inventories fail. Missing names stay
missing; no name, base-card ID or concept match is guessed. SV1 remains frozen;
there is no refresh implementation. Unrecognized additional face layouts fail
rather than being silently omitted. The SV1 card-page batch is not a fallback for
missing API names in this implementation.

`select_name()` prefers adopted SVWB names over SV1. Same-character/name-only,
unadopted, missing-language and unlocated face candidates cannot supply a name.
`populate_name_translation()` materializes a JP-to-Traditional-Chinese name only,
with exact frozen name evidence, deterministic IDs/52-bit revisions and the actual
review date. It preserves digital origin/authority and cannot supply an effect
translation. Ambiguous source names require an adopted context assignment. No
`current` or wording-ready filter discards published identity; callers compose with
`TextPlan.publication_identity()`, including pending wording and errata observations.

Context IDs describe source text and semantic variants, not owner eligibility.
A context hit does not make a digital translation usable for another card or face.
The #53 use/binding layer must revalidate the owner's adopted same-card, exact-face
link and frozen name evidence (through `populate_name_translation()`), then bind
only its returned translation ID to that owner. It must not select official names
by context alone or decode ownership from a render hash. Same-name owners without
an eligible link return no translation even when that context already has one.

Template rendering, regional source exceptions, binding/use, selection, public
export and English term adoption belong to later work. Vocabulary choices are
recognized by the strict wire/loader but glossary population rejects them until
the label projection is implemented; this rollback is atomic. The optional schema
capability does not claim complete importer/validator readiness. Real migration
counts, missing evidence and contract questions are reported separately in review
artifacts; synthetic tests are not production adoption receipts.

The opt-in `translation_names` schema now adds the complete `translation_use`
and `translation_selection` column sets from build-db §9. Its population and
query checks currently accept only name fields with a null ordinal, a face
revision or a complete printing/face pair, and a default context. Other owners
remain represented in the DDL and fail closed at the name-only capability.
Non-default uses require the later application/binding composition; loading a
verified assignment alone does not enable them.

`name_build` reads each owner's own language, exact content hash and source unit.
Unknown or omitted printed text yields no context or use even if a current name
is available; known historical printing names can differ from current names.
Pending wording does not suppress a known name. These helpers consume an already
verified publication database; the digital-name application separately validates
raw archives, policy inputs and verified publication candidates. Full glossary replay remains mandatory.

The complete indexed entry now also accepts name-only `context_assignment` and
`card_name_concept` histories under `translations/overrides/`. Both require the
maintainer's actual sampled/confirmed decision; glossary delegation cannot grant
these exceptions. All predecessors and withdrawn records retain their exact
source evidence and immutable identity checks. Unsupported override kinds still
fail before import. Loader diagnostics locate invalid fields without echoing
their imported values.

`name_replay` validates immutable Git registry bytes, all printing observations
in the explicitly supplied frozen batches, physical face locations and revision
owners. Runtime changes do not invalidate an immutable historical source recipe.
Nonempty identity transitions currently fail closed until complete effective
transition evidence can be composed. Revision assignments must reproduce their
owner from the complete uncorrected observation; corrected revisions cannot be
guessed from a name hash alone. Assignment and concept records each require their
own immutable `identity_basis`; missing/null assignment pins fail before replay.
All histories use their own registry bytes, including superseded/default records.
The reader verifies that each basis is equal to or an ancestor of the explicitly
pinned consumer revision. It never reads a mutable main ref or requires a PR's
consumer to have been merged.

`verify_name_adoption_base(inputs, base_revision)` is a separate pre-adoption
check. Its caller supplies the trusted, full PR base SHA, independently of the
consumer. It checks every override background against that immutable base;
a background exclusive to the feature history cannot be adopted even if the
reader can replay it. The helper does not fetch, infer main or automatically
wire itself into CI. Its CLI is available for the required trusted pre-merge check
below. Ancestry checks supplement, never replace, complete evidence
replay. Formal release builds remain governed by the publication
process. No formal assignment or concept association is created by these APIs.

Identical frozen observations are parsed once across bases in one replay.
The cache includes the full expected observation, exact URL, sealed store/batch
membership and pinned parser recipe. Every basis keeps its own observation uses
and authored/decision audit; every owner still rechecks its source-face mapping.
Historical printing proofs establish the printing's own frozen name, without
reconstructing historic errata coverage. Current resolution separately checks
known printed state, exact language/name and the same permanent parent card/face;
a repaired/reparented owner cannot inherit the old assignment. Non-name fields
remain unsupported until their complete field/ordinal source replay is available.

Complete replay needs frozen card batches for every region present in the
registry. Offline recipes declare both batches; other callers must cover both
through the supplied override references. Both JP and EN physical descriptors
have kind `card`, while digital catalogues have kind `api`. The observation
closure checked here is physical printing identity, including the registry's
cross-region references. It does not replace the independent evidence checks
for ancillary related-card suggestions or source corrections.

`NameReplay.resolve()` rechecks the current owner's confirmed identity, language,
exact name hash and known printed state. A unique exact Japanese concept needs
no exception record. A true homonym needs an adopted semantic assignment; English
names need an explicit concept association. Withdrawal restores mechanical
resolution, and stale hashes cannot transfer an old assignment to a renamed
owner. The returned term/variant and audit IDs are inputs to the owner-local application
layer; replay alone does not populate translations or uses. The offline build independently
replays the full source-use closure, including identity observations; import
audits preserve the corresponding registry and decision evidence.
Each override decision audits only its own immutable identity revision, rather
than claiming the historical basis of unrelated records.

Only reviewed unofficial translations can enter the shared selection table.
Official names need per-owner eligibility and direct `FieldTranslation` bindings;
both the helper and commit-time checks prevent a shared selection from granting
that eligibility. The optional tables do not change the default compiled table
set or snapshot format. Disposable databases use schema version 5 and the existing
atomic rebuild path; failed reconstruction preserves the previous file bytes.

Glossary adoption format 1 now requires explicit `adoption_review` on all four
glossary kinds, `source_claim` on choices, and the mutually exclusive frozen
versus authored Japanese source fields on concepts. Old incomplete payloads are
rejected; changing required fields under format 1 is only safe before the first
formal adoption. Project claims retain their stated work/URLs and note in hashed
authored bytes and never grant official authority or trigger requests.

`delegated_glossary` receipts bind the maintainer delegation, exact record-key
scope, actual coordinator and event. Shards cannot mix review modes; delegated
batches must be confirmed with every member checked. All scope members must
exist in the complete input and carry the same receipt. This exception belongs
only to glossary records, does not relax other adoption loaders, and contributes
to `delegated_glossary_rows` rather than `human_sampled_rows`. Receipt contents
are explicit adoption declarations, not cryptographic proof of a person's
identity or external authorization.

`glossary_emphasis_choice` is a separate, predecessor-checked history restricted
to rule terms. `Snapshot.emphasis()` returns the effective Bool or None plus
its adopted record hash/decision; None means missing emphasis, including
withdrawal. Display the available text without bold and report missing emphasis;
it is neither an adopted false nor a reason to fall back an entire context.
Other glossary categories derive true from the formal type, never from the key
prefix. Emphasis histories remain auditable in the pinned authored records; no
new database table or public snapshot field is introduced.

This change supports rawless project concept/choice import and emphasis audit.
Vocabulary choices still stop at the existing atomic label-projection boundary;
full label use/binding and public raw/translated spans need the later projection
work. Loader validation of all entries is not a claim that 279 terms have been
adopted or projected.

Before adopting a real name assignment or concept association, the coordinator or
reviewer must supply a trusted immutable PR base SHA and execute:

```bash
sve-carddb translations check-name-adoption-base \
  --authored /absolute/checkout/authored \
  --repository /absolute/checkout \
  --authored-revision <full-consumer-sha> \
  --base-main-revision <independently-trusted-full-pr-base-sha>
```

This checks every override basis against the explicit base history, separately
from full frozen replay and the reader's consumer-ancestry check. A feature branch
cannot select its own trusted main base. Incomplete shallow ancestry produces an
unavailable diagnostic, rather than claiming a proven nonancestor. Full history
must be supplied before a negative ancestry conclusion is accepted.
`IdentityEvidence.costs()` reports registry bases, complete observation cache
keys, cache hits, new parses and preserved per-basis observation uses. Unchanged
observations can be reused; changed expected observations or sealed batches require
new verification and their cost must be measured separately.

The context-v1 and use-v1 recipes remain shared with legacy name materialization.
Nondefault contexts require the actual owner's complete assignment replay and
imported identity audit. Ordinary render IDs depend on the exact source/target
name, origin, game and recipe; adding later concept associations, policy versions,
receipt metadata or another owner does not change them. True semantic variants
remain distinct contexts. `translation_names` remains the sole owner of the
use/selection DDL; extending it to other fields requires their complete source
checks, rather than registering the same tables under a second capability.
