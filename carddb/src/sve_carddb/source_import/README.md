# Offline source registration

This module implements the isolated registration boundary in
[construction adoption §2](../../../../docs/schema/construction-adoption.md#2-一次性抓回原檔如何正式登錄與釘版).
It reads an already acquired set without creating an HTTP client or opening the
configured live manifest. Registration does not adopt rule meanings, prove source
coverage, extract CR clauses, or replace sealing and independent backup checks.

## Inputs and program pins

The input directory must include `index.jsonl` and content-addressed raw files
under `raw/<64 lowercase hex>`. Each JSONL observation has exactly these fields:
`url`, `final_url`, `status`, `sha256`, `bytes`, `fetched_at`, `content_type`, `etag`,
`last_modified`, `chain`. Each chain hop has `url`, `status`, `location`.
The terminal status must be 200; `location` is null on its last hop. Intermediate
redirects must resolve to the following observed URL. No redirects are fetched.
Requested, final and hop URLs must use HTTPS and the exact official JP or EN host.
All observations are required, including news indexes and historical announcements.
Unknown fields, duplicate requested identities, orphan raw files, symlinks, changed
bytes, missing metadata and unsupported types stop registration without cleaning
the input set. A shared raw hash can support multiple distinct URL identities.

`fetched_at` must be the observed full UTC instant. The original index bytes,
headers and chain are retained verbatim. This timestamp populates all three
Resource fetch/change/check times; registration records its own separate clock.
It never guesses official publication or effective dates.

An explicit UTF-8 JSON selection contains exactly:

| Field | Value |
| --- | --- |
| `source_import_format` | Integer `1` |
| `sources` | Nonempty unique `{url,provider,kind}` items covering the complete index |
| `program_revision` | Full immutable 40-hex Git HEAD of the registration program |
| `dependencies` | Sorted unique `{name,sha256}` pins for exactly `PROGRAM_FILES` |

URLs in `sources` are canonical **requested** identities. Provider is `jp` or
`en`; kind is `rules`, `limit` or `news`. HTML purposes match the regional
`/rules/`, `/card_limit/` or `/restrictions/`, and `/news/` paths. A PDF must be
explicitly classified as `rules`, with a PDF URL path. Region is never guessed
from the PDF host: a JP-hosted EN PDF can be explicitly classified as EN.

Prepare `purposes.json` as the reviewed `sources` array, then generate the
selection from a committed checkout at the repository root, for example:

```bash
uv run --directory carddb python - <<'PY'
import json
import subprocess
from pathlib import Path
from sve_carddb.core.json import digest
from sve_carddb.source_import.importer import PROGRAM_FILES

revision = subprocess.check_output(["git", "rev-parse", "HEAD"], text=True).strip()
selection = {
    "source_import_format": 1,
    "sources": json.loads(Path("purposes.json").read_text(encoding="utf-8")),
    "program_revision": revision,
    "dependencies": [
        {"name": name, "sha256": digest(Path(name).read_bytes())}
        for name in PROGRAM_FILES
    ],
}
with Path("selection.json").open("x", encoding="utf-8") as output:
    json.dump(selection, output, sort_keys=True)
PY
```

Keep acquisition data and these task selections outside Git. The registrar verifies
the listed dependency files against their **executing** bytes and pinned Git blobs, and
requires the supplied repository HEAD to equal `program_revision`. Changed or
incomplete listed pins fail before publication. This is a fixed file list, not the
complete transitive import or execution closure; changes outside that list are not
checked by this boundary. No new third-party dependency is used.

## Check, register and retain

```bash
uv run --directory carddb sve-carddb source-import register \
  --input-dir /absolute/acquired-set \
  --selection /absolute/selection.json \
  --program-root /absolute/committed-checkout \
  --output /absolute/new-isolated-data
```

The default is `--check`: validate the input, destination and code pins, and print
only file count, total raw bytes and receipt identity. It creates no dataset or
manifest. Execute only an authorized offline registration by adding `--execute`.
Failures report fixed import/manifest reasons, validation field locations and types,
or I/O exception classes; they do not print input values or private filesystem paths.
The output must be an absolute path without symlinks or traversal, outside the
input set, selection file and configured `SVE_DATA_DIR`. Do not point
`SVE_DATA_DIR` at the output while registering or rerunning registration.

Execution copies unchanged HTML/PDF into URL-keyed regional paths and publishes
the complete directory with a no-replace rename. Its manifest has `user_version=2`,
an exact-index import receipt and precisely matching Resources, with no fabricated
HTTP request log or discovery events. Indexes/selections are capped at 1 MiB,
individual raw files at 64 MiB; bodies are read one at a time. HTML validation is
a nonempty title/body check plus simple error-title rejection. PDF validation only
checks container signatures; it is **not** PDF parsing, clause extraction or proof
that an announcement supports a claim. Adoption must validate that separately.

A normal exception or interrupt removes only this run's unpublished staging
directory. A process kill or power loss may leave an unpublished `.source-import-*`
sibling: stop and have the coordinator inspect it and any published output before
separately authorizing cleanup. The registrar does not resume or repair orphaned
work. Rerunning the exact receipt checks the published marker, frozen manifest and
every raw hash before returning `reused`; it preserves bytes and `registered_at`.
Conflicting or unmarked existing outputs are rejected before SQLite is opened.

Use the existing archive workflow explicitly with the isolated dataset after
registration: seal, independent backup, then restore-check. Retain the original
acquisition and registration inputs until that closure is independently recoverable.
This command performs none of those operations automatically. Readers validate
the complete v2 schema, receipt and Resource correspondence. The crawl writer
continues supporting v1 only and refuses v2; existing sealed v1 batches remain
readable. Inventory format and raw/source/version ID recipes are unchanged.
The complete v2 DDL is frozen separately in `manifest_schema_v2.py`; live v1 schema
evolution does not redefine it. Golden tests pin its canonical schema signature and
the receipt identity recipe independently of the executing checkout.

Tests use tiny synthetic HTML/PDF and an immutable module-scoped synthetic program
repository. They exercise isolation, conflicting writes, interruption, v1/v2
compatibility, and seal → backup → restore without official source content.
