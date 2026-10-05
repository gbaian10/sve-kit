# Digital-name policy inputs

This module reads the editable name and link policies defined by
`docs/schema/digital-name-policy.md`. Its loader and report command do not write
applications or receipts. The explicit offline name application is described
below.

`load(authored, repository, authored_revision)` requires a full immutable Git SHA,
regular indexed files, exact checkout bytes and complete closure. The index lists
one `current.yaml` per policy. Names hold business conditions and quality fields.
Links hold only the frozen digital catalogue batches and the editable name and
card-target exclusion lists; the same-name rule itself is fixed in code
(`evaluate.LINK_RELATION`, `evaluate.LINK_GAMES`). Changing the rule is a code
change reviewed in a PR. Approval receipts are not an input or a loading gate.

`require_runtime()` checks the currently loaded evaluator and source dependency
bytes against the build context.

`current_evaluate.catalogue()` and `link_catalogue()` reuse complete frozen API
inventory traversal. Null or missing translations remain members; uniqueness is
never built from selected IDs. `link_catalogue()` also refuses exclusions that do
not locate a frozen Japanese name or card target.
`owner_text()` checks a JP owner's registry parents, source-face map, full
identity observation and exact name field and returns frozen `OwnerEvidence`.
`name_result()` consumes that checked evidence and returns `NamePolicyResult`.
`rule_links()` produces separate card-level `RuleLinkPlan` values. They are not
human links, have no review authority and cannot supply a name. Different
build contexts cannot be combined. The composing build must still validate its
real owner revision/printed state and complete input use closure before applying
any result; the explicit name application below supplies that integration.

## Offline private report

```bash
sve-carddb digital-name-policies report \
  --authored /absolute/checkout/authored \
  --authored-revision <full-retained-main-sha> \
  --repository /absolute/checkout \
  --context /absolute/private/current-context.json \
  --store <store-id>=/absolute/archive \
  --baseline-empty \
  --output /absolute/private/report.json
```

The context is an existing `BuildContext` with the complete current runtime
closure and exactly `catalog_registry`, `digital_link_sources` and
`translation_recipes` in its configuration. Names use the context's batches;
link plans use the links policy's own batches. All stores are read-only; no
network or live manifest is used. Output must be absolute, not symlinked, and
disjoint from all input roots. It replaces a report only after the complete
report is written.

Use `--baseline <previous-report.json>` instead of `--baseline-empty` to report
new frozen source observations. Optional `--compare-context <context.json>`
reports changes in a newer complete digital catalogue without activation.
Unknown files or fields are refused, not treated as an empty entry.

Diagnostics contain IDs, source references, original card numbers, hashes and
conditions only; there is no option to print official names. Owner counts describe
frozen source observations through registry mappings, not published DB revisions
or printed-text verification. Multiple source batches can describe one physical
printing separately. Browsing plans are before human precedence/suppression;
they are never published links. Coverage remains unadopted and publication
counts remain zero. Official-name research exports, if needed, belong to private
scratch tooling outside this module and git.

## Owner-local name application

Current names use one format-two policy plus the current glossary and explicit
owner/source overrides. `Inputs.configuration()` pins the complete authored
closure; application checks current runtime bytes and whole frozen catalogues.
Each published JP face revision and known printing face uses its own verified
`TextPlan` observation. Unknown printed names remain unavailable.

Names prefer an explicit owner/source override, then eligible current policy
wording, then the owner's same-card human link, then a current glossary choice,
with original text as the final fallback. Official choices are independently
rechecked against their own exact face and frozen name evidence. Exclusions block
automatic names; sharing a context never grants eligibility. Origin and low
confidence stay separate from human links. The application produces current
render values and owner bindings, without inventing approval events or review
decisions. Reports contain IDs, counts and reasons, not official card wording.

Public snapshot projection is a separate coordinated boundary and must preserve
current `origin` and `low_confidence` without claiming human review.
