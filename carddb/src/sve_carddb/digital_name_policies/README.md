# Frozen digital-name policy inputs

This module validates the complete portable policy entry defined by
`docs/schema/digital-name-policy.md`. It does not populate a database, create
applications or receipts, or activate policies in preview/snapshot builds.

`load(authored, repository, authored_revision)` requires a full immutable Git SHA,
regular indexed files, exact checkout bytes and complete policy/approval/exclusion
history. It keeps exact byte pins and detached canonical envelopes. The two
already adopted documents and receipts have immutable bindings. The supported
v1 rule signature covers the complete operational wording, scope, matcher and
answers, with source pins and independently approved initial lists checked
separately. New semantics require reviewed implementation support. Hash
consistency alone does not prove that a maintainer actually approved a document.
Private original pages and projection evidence are not re-read by CI: their
adoption review is the boundary specified by the contract.

`historical_sources()` verifies historical parser/config/registry pins against
immutable Git blobs. `require_runtime()` independently checks the currently
loaded evaluator and source dependency bytes. A current comment or refactor
does not invalidate the retained historical policy. Declared commits must remain
available in the repository; use retained main commits for formal input pins.

`catalogue()` reuses complete frozen API inventory traversal. Null or missing
translations remain members; it does not build uniqueness from selected IDs.
`owner_text()` checks a JP owner's registry parents, source-face map, full
identity observation and exact name field and returns frozen `OwnerEvidence`.
`name_result()` consumes that checked evidence and returns `NamePolicyResult`.
`rule_links()` produces separate card-level `RuleLinkPlan` values. They are not
human link adoptions, have no review authority and cannot supply a name. Different
build contexts cannot be combined. The composing build must still validate its
real owner revision/printed state and complete input use closure before applying
any result; that database integration belongs to the later implementation.

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
`translation_recipes` in its configuration. It does not select or replace the
policy catalogue. All stores and policy histories are read-only; no network or
live manifest is used. Output must be absolute, not symlinked, and disjoint from
all input roots. It replaces a report only after the complete report is written.

Use `--baseline <previous-report.json>` instead of `--baseline-empty` to report
new frozen source observations. Optional `--compare-context <context.json>`
reports changes in a newer complete digital catalogue without activation. The
continuation, exclusion-log and rule-resume formats are deliberately unsupported.
Unknown files or fields are refused, not treated as an empty entry.

Diagnostics contain IDs, source references, original card numbers, hashes and
conditions only; there is no option to print official names. Owner counts describe
frozen source observations through registry mappings, not published DB revisions
or printed-text verification. Multiple source batches can describe one physical
printing separately. Browsing plans are before human precedence/suppression;
they are never published links. Historical private warning labels are unavailable
and explicitly reported as such. Coverage remains unadopted and publication
counts remain zero. Official-name research exports, if needed, belong to private
scratch tooling outside this module and git.
