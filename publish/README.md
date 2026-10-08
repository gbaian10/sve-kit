# sve-publish

`sve-publish upload` uploads the public root that `snapshot export-offline` wrote
(`--preview-dir`). It reads `snapshots/preview/current.json`, takes that manifest's
closure and its referenced images, and nothing else. There is no separate release
bundle, ledger, checkpoint, reservation or writer lease: one maintainer runs the
export and then the upload on one machine. No credentials or account, bucket or
CDN hostname are stored in the repository or passed through CI.

The data versions are `preview-…`, so the uploaded index entry is a development
entry. Formal releases still need the gates tracked by #34; renaming a preview
does not make it formal.

## What is uploaded

- `snapshots/manifests/<sha256>.json` and its `.gz`, plus `.br` when the export
  wrote one;
- every payload, the text union and changes listed by the manifest, with the
  `.gz`/`.br` siblings it declares;
- `images/<size>/<int_id>[-f<ordinal>].webp` for each available, approved
  printing face size listed in the manifest's media rows;
- `snapshots/versions/index.json`, written last.

Older blobs left in the export root, the private directory (`--private-dir`:
inputs, reports, media state), the DB bundle, the image library and recipe cache
are never read for upload. Before any credential or HTTP access, the command
checks the pointer's manifest hash, the schema, every payload hash and length,
gzip/Brotli decoding, the reader join and each image's byte count, WebP format
and dimensions. Symlinks and special files are refused. The join resolves the
printing/face/media rows used to select image keys and checks their reference
closure. Encoded siblings have no independent hash, so decoding checks that
they carry the verified raw bytes; upload never recompresses them. The writer
already checks the text union's equivalence to the shards. Upload checks its
manifest hash and length without running a second full reader join.

Headers come from the contract, not from file extensions:

| Object | Content-Type | Cache-Control | Content-Encoding |
| --- | --- | --- | --- |
| JSON | `application/json` | `public,max-age=31536000,immutable` | none, `gzip` or `br` for siblings |
| WebP | `image/webp` | `public,max-age=86400,must-revalidate` | none |
| index | `application/json` | `no-store` | none |

## Dry-run

```bash
uv --directory publish run sve-publish upload \
  --export-dir /explicit/preview-root --dry-run
```

`--export-dir` can read `SVE_EXPORT_DIR`; the CLI option takes precedence.
The root has no default and must be a non-empty absolute path.

Dry-run is the default. It reads no credentials, opens no HTTP client and writes
nothing. It prints the data version, manifest hash and local JSON/image counts
and bytes; how many objects already exist remotely is unknown offline.

## Execute

```bash
uv --directory publish run sve-publish upload \
  --export-dir /explicit/preview-root \
  --account-id "$R2_ACCOUNT_ID" --bucket "$R2_DEV_BUCKET" \
  --skip-cdn-verify --execute
```

Target selection uses explicit `--account-id`/`--bucket`, or `R2_ACCOUNT_ID`/
`R2_DEV_BUCKET` in execute mode. Credentials are read only from
`SVE_R2_ACCESS_KEY_ID` and `SVE_R2_SECRET_ACCESS_KEY`; missing values fail
without HTTP. No env file, profile, metadata service or ambient AWS credentials
are read. Agents and CI do not run this against a real service.

The steps are:

1. Read the remote index and decide the new one: first upload gets revision 1
   and `previous=null`; later uploads get the next revision and the old
   `current` as `previous`. An export that is already current changes nothing.
   A different manifest under the current data version, or an export older than
   current, is refused.
2. For each image, then each JSON object, GET the remote object. Equal bytes and
   headers count as read back. Otherwise PUT (`If-None-Match: *` for a new key,
   `If-Match` for an image overwrite) and GET it again to compare bytes and
   headers. Immutable JSON with different bytes or headers stops the run; it is
   never overwritten.
3. CDN check: two ordinary GETs of each full image URL with its `?v=` against
   `--cdn-base-url`, without authorization, redirects, cache bypass or purge.
   `--skip-cdn-verify` skips only this step (development behind Access, checked in
   a browser); the output marks it as skipped.
4. Check that the index is unchanged, write it with a conditional PUT, and read
   it back.

An unchanged rerun makes no PUT. After a failure, current is unchanged; run the
same command again to upload what is missing. A failed image overwrite can leave
new origin bytes under a key that the old current still references; fixed image
keys keep only current bytes (ADR-0015). Do not run an upload while an export
writes the same root: `export-offline` removes the pointer before it overwrites
a changed image, so an interrupted export has to be rerun before uploading.

The SDK applies SigV4 to the exact headers, payload and sorted query parameters.
Unsupported conditions/statuses, redirects and transport failures stop, with no
plain-PUT fallback or write retry. GET keeps compressed siblings' wire bytes.
Both clients disable proxies/ambient configuration and redirects; each I/O has
a 30 second timeout. Responses are bounded to 128 MiB per object and 2 MiB per
inventory page. Errors omit response bodies, request headers, credentials and
signatures. Do not enable HTTP wire logging or `--showlocals` around execution.

## Collection

```bash
uv --directory publish run sve-publish gc \
  --account-id "$R2_ACCOUNT_ID" --bucket "$R2_DEV_BUCKET" --dry-run
```

`gc` reads the remote index and keeps the index, the JSON closures of
`current` and `previous` (plus their manifest `.br` if present) and the images of
`current` only (ADR-0016). `changes.from` never retains a third version.
The dry-run lists every other object under `snapshots/blobs/`,
`snapshots/manifests/` and the five image sizes; `--namespace` limits the list to
some of those prefixes. `--execute` deletes them with plain DELETE (R2 does not
promise `If-Match` on DeleteObject), checking before each one that the index has
not changed. An irregular key in those prefixes, a missing retained object or a
missing index stops it. Collection never runs automatically and must not run
while an upload is in progress, because objects not yet in the index would be
collected.

Real R2 behavior, deployed Cache Rules, query-key separation, Access, browsers
and the service worker still need separate checks by the maintainer. Purge is a
separate permission and is absent here.

## Development

```bash
uv --directory publish sync
uv --directory publish run ruff check
uv --directory publish run ruff format --check
uv --directory publish run mypy
uv --directory publish run pytest -n 4 --cov
```

The editable path dependency on `../carddb` supplies one public reader and
validator: production code imports only `sve_carddb.export.read_api`. Synthetic
test fixtures use the real exporter to generate temporary previews, without
reading authored data or private test data. The test session independently
blocks external networking and writes outside pytest temporary roots, except
coverage and bytecode artifacts. SDK fake clients are closed after every test.
Combined line and branch coverage is required to reach 92% independently of
carddb, including fork PRs.

| Variable | Purpose | Required / default |
| --- | --- | --- |
| `SVE_EXPORT_DIR` | Public preview root; overridden by `--export-dir` | Upload requires CLI or env; no default |
| `R2_ACCOUNT_ID` | R2 account; overridden by `--account-id` | Remote operations only; no default |
| `R2_DEV_BUCKET` | R2 bucket; overridden by `--bucket` | Remote operations only; no default |
| `SVE_R2_ACCESS_KEY_ID` | R2 access credential | Remote operations only; no default |
| `SVE_R2_SECRET_ACCESS_KEY` | R2 secret credential | Remote operations only; no default |
