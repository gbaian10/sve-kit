# Reviewed SVE card inputs

These UTF-8 JSON files preserve Japanese and English SVE card-page observations
and their source evidence. Each card lists readable `documents`, followed by
minimal source references; identical documents share a stable reference within
that card. Adjacent `.evidence.json` files hold build evidence with shared tables.
Batch source selections live separately under `batches/`, in fixed groups of
1,000 source versions. New batches leave existing card and evidence files unchanged
when no source observation changes. Build integration is still planned; players
and the simulator continue to consume versioned carddb snapshots.

The initial observations come from official Japanese and English SVE card pages.
Each records its exact source URL, raw SHA-256, observation time, parser recipe
and declared producer commit. Raw archives and backups remain separately preserved.
Private inventory timestamps, manifest fingerprints and per-batch receipts are
not published. No local paths, archive store names, credentials, image bytes or
audio bytes are included.

Read the [input contract](../docs/schema/ingest/parsed-card-inputs.md) before
regenerating this tree. All families use fixed ranges of 100 card numbers; letter
prefixes have separate ASCII paths. The index pins every JSON member. Existing
observations, batch selections and filing must survive every update. Validate
against the PR base data tree; a self-consistent hash alone does not protect history.

Our contributions to structure and provenance follow the path terms in
[LICENSING.md](../LICENSING.md). Official names, card wording, flavor text, Q&A,
product wording, illustrator credits and other source-derived expression are
excluded from the project's license grants. Private archival access and public
repository inclusion do not establish permission from the rights holders.
